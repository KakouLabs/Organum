use rayon::prelude::*;

use crate::resampler::{common::consts, device, device::Device, synthesis};

#[inline]
fn relax_warp_factor_for_high_pitch(
    total_factor: f32,
    target_base_f0: f32,
    profile: synthesis::QualityProfile,
) -> f32 {
    if profile.high_pitch_warp_relax <= 0.0 || target_base_f0 <= 523.25 {
        return total_factor;
    }

    let relax = ((target_base_f0 - 523.25) / 500.0).clamp(0.0, profile.high_pitch_warp_relax);
    1.0 + (total_factor - 1.0) * (1.0 - relax)
}

pub fn apply_warp_and_tilt(
    sp_render: &mut world::native::MatrixF32,
    sample_rate: u32,
    render_length: usize,
    total_factor: f32,
    target_base_f0: f32,
    device: Device,
    quality_preset: crate::config::QualityPreset,
) {
    const GPU_WARP_MIN_SAFE_FRAMES: usize = 64;

    let profile = synthesis::QualityProfile::from_preset(quality_preset);

    let total_factor = relax_warp_factor_for_high_pitch(total_factor, target_base_f0, profile);

    let (do_tilt, tilt_intensity, fft_size_half, nyquist) = if target_base_f0 > 350.0 {
        (
            true,
            ((target_base_f0 - 350.0) / 450.0).clamp(0.0, profile.high_pitch_tilt_cap),
            (consts::FFT_SIZE / 2) as f32,
            (sample_rate / 2) as f32,
        )
    } else {
        (false, 0.0, 1.0, 1.0)
    };

    let warp_lut = if (total_factor - 1.0).abs() > 0.001 {
        let sp_len = sp_render.cols();
        if sp_len > 0 {
            Some(synthesis::WarpLut::cached(
                sp_len,
                sample_rate as f32,
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
    let tilt_factors: Option<Vec<f32>> = if do_tilt {
        const TILT_START_HZ: f32 = 4200.0;
        const TILT_SPAN_HZ: f32 = 5500.0;
        const PRESENCE_START_HZ: f32 = 2400.0;
        const PRESENCE_SPAN_HZ: f32 = 2600.0;

        let sp_len = sp_render.cols();
        let factors: Vec<f32> = (0..sp_len)
            .map(|d| {
                let freq = (d as f32 / fft_size_half) * nyquist;
                let tilt = if freq > TILT_START_HZ {
                    let freq_scale: f32 = (freq - TILT_START_HZ) / TILT_SPAN_HZ;
                    1.0 / (1.0 + tilt_intensity * profile.tilt_strength * freq_scale.powi(2))
                } else {
                    1.0
                };

                let presence = if target_base_f0 > 500.0 && freq > PRESENCE_START_HZ {
                    let freq_scale =
                        ((freq - PRESENCE_START_HZ) / PRESENCE_SPAN_HZ).clamp(0.0, 1.0);
                    1.0 + tilt_intensity
                        * profile.presence_max_gain
                        * (1.0 - (freq_scale - 0.5).abs() * 2.0).max(0.0)
                } else {
                    1.0
                };

                (tilt * presence).clamp(0.82, 1.08)
            })
            .collect();
        Some(factors)
    } else {
        None
    };

    const PAR_THRESHOLD: usize = 2048;

    let rows = sp_render.rows();
    let cols = sp_render.cols();

    if let Some(ref lut) = warp_lut {
        let mut gpu_applied = false;
        if matches!(warp_backend, synthesis::WarpBackend::Gpu)
            && render_length >= GPU_WARP_MIN_SAFE_FRAMES
        {
            match synthesis::try_apply_warp_batch_with_backend(
                sp_render.as_mut_slice(),
                rows,
                cols,
                lut,
                warp_backend,
            ) {
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
            synthesis::apply_warp_cpu_batch(sp_render.as_mut_slice(), rows, cols, lut);
        }
    }

    let apply_sp_tilt = |sp: &mut [f32]| {
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
        sp_render
            .as_mut_slice()
            .chunks_exact_mut(cols)
            .for_each(apply_sp_tilt);
    } else {
        sp_render
            .as_mut_slice()
            .par_chunks_exact_mut(cols)
            .for_each(apply_sp_tilt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_pitch_warp_relaxation_moves_toward_neutral() {
        let profile =
            synthesis::QualityProfile::from_preset(crate::config::QualityPreset::Balanced);
        let low = relax_warp_factor_for_high_pitch(1.2, 440.0, profile);
        let high = relax_warp_factor_for_high_pitch(1.2, 880.0, profile);

        assert!((low - 1.2).abs() < 1e-6);
        assert!(high < 1.2);
        assert!(high > 1.0);
    }
}
