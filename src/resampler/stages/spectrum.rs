use rayon::prelude::*;

use crate::resampler::{
    common::consts,
    synthesis,
};

pub fn apply_warp_and_tilt(
    sp_render: &mut [Vec<f64>],
    sample_rate: u32,
    render_length: usize,
    total_factor: f64,
    target_base_f0: f64,
    gpu_warp_enabled: bool,
    gpu_warp_min_frames: usize,
) {
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
            Some(synthesis::WarpLut::new(sp_len, sample_rate as f64, total_factor))
        } else {
            None
        }
    } else {
        None
    };

    let warp_dispatch = synthesis::WarpDispatchConfig {
        gpu_warp_enabled,
        gpu_warp_min_frames,
    };
    let warp_backend = warp_dispatch.choose_backend(render_length);
    if warp_lut.is_some() {
        tracing::debug!(
            "warp backend: {:?} (gpu_warp_enabled={}, gpu_warp_min_frames={}, render_length={})",
            warp_backend,
            gpu_warp_enabled,
            gpu_warp_min_frames,
            render_length,
        );
    }
    let tilt_factors: Option<Vec<f64>> = if do_tilt {
        let sp_len = sp_render.first().map(|f| f.len()).unwrap_or(0);
        let factors: Vec<f64> = (0..sp_len)
            .map(|d| {
                let freq = (d as f64 / fft_size_half) * nyquist;
                if freq > 3500.0 {
                    let freq_scale: f64 = (freq - 3500.0) / 4000.0;
                    1.0 / (1.0 + tilt_intensity * 2.0 * freq_scale.powi(2))
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
        if matches!(warp_backend, synthesis::WarpBackend::Gpu) {
            match synthesis::try_apply_warp_batch_with_backend(
                sp_render,
                lut,
                warp_backend,
            ) {
                Ok(()) => {
                    gpu_applied = true;
                }
                Err(e) => {
                    tracing::warn!("GPU warp failed, falling back to CPU: {}", e);
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
