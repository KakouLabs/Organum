use rayon::prelude::*;

use crate::resampler::{common::consts, device, device::Device, synthesis};

pub fn apply_warp_and_tilt(
    sp_render: &mut [Vec<f64>],
    sample_rate: u32,
    render_length: usize,
    total_factor: f64,
    target_base_f0: f64,
    device: Device,
) {
    const GPU_WARP_MIN_SAFE_FRAMES: usize = 64;

    let (do_tilt, tilt_intensity, fft_size_half, nyquist) = if target_base_f0 > 350.0 {
        (
            true,
            ((target_base_f0 - 350.0) / 400.0).clamp(0.0, 1.0),
            (consts::FFT_SIZE / 2) as f64,
            (sample_rate / 2) as f64,
        )
    } else {
        (false, 0.0, 1.0, 1.0)
    };

    let warp_lut = if (total_factor - 1.0).abs() > 0.001 {
        let sp_len = sp_render.first().map(|f| f.len()).unwrap_or(0);
        if sp_len > 0 {
            Some(synthesis::WarpLut::new(
                sp_len,
                sample_rate as f64,
                total_factor,
            ))
        } else {
            None
        }
    } else {
        None
    };

    let warp_backend = device.as_warp_backend();
    if warp_lut.is_some() {
        tracing::debug!(
            "warp backend: {:?} (render_length={})",
            warp_backend,
            render_length,
        );
    }
    let tilt_factors: Option<Vec<f64>> = if do_tilt {
        const TILT_START_HZ: f64 = 4200.0;
        const TILT_SPAN_HZ: f64 = 5500.0;
        const TILT_STRENGTH: f64 = 1.2;

        let sp_len = sp_render.first().map(|f| f.len()).unwrap_or(0);
        let factors: Vec<f64> = (0..sp_len)
            .map(|d| {
                let freq = (d as f64 / fft_size_half) * nyquist;
                if freq > TILT_START_HZ {
                    let freq_scale: f64 = (freq - TILT_START_HZ) / TILT_SPAN_HZ;
                    1.0 / (1.0 + tilt_intensity * TILT_STRENGTH * freq_scale.powi(2))
                } else {
                    1.0
                }
            })
            .collect();
        Some(factors)
    } else {
        None
    };

    const PAR_THRESHOLD: usize = 2048;

    if let Some(ref lut) = warp_lut {
        let mut gpu_applied = false;
        if matches!(warp_backend, synthesis::WarpBackend::Gpu)
            && render_length >= GPU_WARP_MIN_SAFE_FRAMES
        {
            match synthesis::try_apply_warp_batch_with_backend(sp_render, lut, warp_backend) {
                Ok(()) => {
                    gpu_applied = true;
                }
                Err(e) => {
                    tracing::warn!("GPU warp failed, falling back to CPU: {}", e);
                    device::mark_gpu_unavailable(&format!("warp stage error: {}", e));
                }
            }
        }

        if !gpu_applied {
            synthesis::apply_warp_cpu_batch(sp_render, lut);
        }
    }

    let apply_sp_tilt = |sp: &mut Vec<f64>| {
        if let Some(ref tilt) = tilt_factors {
            for (d, s) in sp.iter_mut().enumerate() {
                *s *= tilt[d];
                *s = s.max(1e-16);
            }
        } else {
            for s in sp.iter_mut() {
                *s = s.max(1e-16);
            }
        }
    };

    if render_length < PAR_THRESHOLD {
        sp_render.iter_mut().for_each(apply_sp_tilt);
    } else {
        sp_render.par_iter_mut().for_each(apply_sp_tilt);
    }
}
