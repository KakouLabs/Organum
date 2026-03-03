use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;
use std::time::Instant;

pub mod audio;
pub mod consts;
pub mod features;
pub mod flags;
pub mod synthesis;
pub mod types;
pub mod utils;

pub use audio::{read_audio, resample_audio, write_audio};
pub use features::{generate_features, read_features, write_features};
pub use types::ResampleRequest;
pub use utils::{interpolate_frames, to_feature_path, LinearInterpolator};

/// WORLD feature만 추출해서 캐시에 저장. 합성은 하지 않음.
pub fn generate_and_cache_features(
    input_path: &Path,
    config: &crate::config::OrganumConfig,
) -> Result<()> {
    let start_time = Instant::now();
    let feature_path = to_feature_path(input_path, &config.feature_extension);

    if feature_path.exists() {
        if read_features(&feature_path).is_ok() {
            tracing::debug!("Cache hit (read-only verification) for {:?}", input_path);
            return Ok(());
        }
        tracing::warn!(
            "Cache corrupted or outdated for {:?}, regenerating...",
            input_path
        );
    } else {
        tracing::debug!("Cache miss for {:?}", input_path);
    }

    let audio = read_audio(input_path, config.sample_rate)?;
    let features = generate_features(&audio, config.sample_rate, config.frame_period)?;
    write_features(&feature_path, &features, config.zstd_compression_level)?;
    tracing::info!(
        "Feature extraction & cache generation complete for {:?} in {:?}",
        input_path,
        start_time.elapsed()
    );
    Ok(())
}

/// 캐시된 feature를 로드(없으면 추출)하고, 플래그에 따라 파라미터를 보간한 뒤 합성.
pub fn resample(req: &ResampleRequest) -> Result<()> {
    let config = crate::config::global_config();
    let sample_rate = config.sample_rate;
    let frame_period = config.frame_period;
    let feat_ext = &config.feature_extension;

    let input_path = Path::new(&req.input_file);
    let output_path = Path::new(&req.output_file);
    let feature_path = to_feature_path(input_path, feat_ext);

    let features = if feature_path.exists() {
        match read_features(&feature_path) {
            Ok(f) => {
                tracing::info!("[CACHE HIT] Loaded features from {:?}", feature_path);
                f
            }
            Err(e) => {
                tracing::warn!(
                    "[CACHE MISS] Corrupted or old cache {:?}: {}. Regenerating...",
                    feature_path,
                    e
                );
                let audio = read_audio(input_path, sample_rate)?;
                let features = generate_features(&audio, sample_rate, frame_period)?;
                let _ = write_features(&feature_path, &features, config.zstd_compression_level);
                features
            }
        }
    } else {
        tracing::info!("[CACHE MISS] No cache found for {:?}", input_path);
        let audio = read_audio(input_path, sample_rate)?;
        let features = generate_features(&audio, sample_rate, frame_period)?;
        let _ = write_features(&feature_path, &features, config.zstd_compression_level);
        features
    };

    let start_synthesis = Instant::now();

    let feature_length = features.f0.len();
    if feature_length == 0 {
        return Err(anyhow::anyhow!("No features found in input file"));
    }

    let fps = 1000.0 / frame_period;
    let feature_length_sec = feature_length as f64 / fps;

    let vuv: Vec<bool> = features.f0.iter().map(|&f0| f0 > 0.0).collect();
    let base_f0_log2 = features.base_f0.log2();
    let f0_off: Vec<f64> = features
        .f0
        .iter()
        .map(|&f0| {
            if f0 == 0.0 {
                0.0
            } else {
                12.0 * (f0.log2() - base_f0_log2)
            }
        })
        .collect();

    let start_sec = req.offset as f64 / 1000.0;
    let end_sec = if req.cutoff < 0.0 {
        start_sec - req.cutoff as f64 / 1000.0
    } else {
        (feature_length_sec - req.cutoff as f64 / 1000.0).max(start_sec)
    };

    let cons_sec = req.fixed_length as f64 / 1000.0;
    let velocity = if req.velocity <= 0.0 {
        100.0
    } else {
        req.velocity as f64
    };
    let cons_stretch = 2.0_f64.powf(1.0 - velocity / 100.0);

    let mut actual_cons_sec = cons_sec.min(end_sec - start_sec).max(0.0);
    let mut scaled_cons_sec = actual_cons_sec * cons_stretch;

    let length_req_sec = req.length_req as f64 / 1000.0;

    if scaled_cons_sec > length_req_sec {
        scaled_cons_sec = length_req_sec;
        actual_cons_sec = scaled_cons_sec / cons_stretch;
    }

    let render_length = (length_req_sec * fps).round() as usize;
    if render_length == 0 {
        return Err(anyhow::anyhow!("Calculated render length is 0"));
    }

    let t_render: Vec<f64> = (0..render_length)
        .into_par_iter()
        .map(|i| {
            let t_out_sec = (i as f64) / fps;
            let t_in_sec = if t_out_sec <= scaled_cons_sec && scaled_cons_sec > 0.0 {
                start_sec + (t_out_sec / cons_stretch)
            } else {
                let vowel_time_out = t_out_sec - scaled_cons_sec;
                let vowel_time_req = (length_req_sec - scaled_cons_sec).max(0.001);
                let vowel_time_src = (end_sec - (start_sec + actual_cons_sec)).max(0.001);
                (start_sec + actual_cons_sec) + vowel_time_out * (vowel_time_src / vowel_time_req)
            };
            t_in_sec * fps
        })
        .collect();

    let f0_off_interp = utils::LinearInterpolator::new(&f0_off);
    let f0_off_render = f0_off_interp.sample_vec(&t_render);
    let vuv_render: Vec<bool> = t_render
        .par_iter()
        .map(|&t| vuv[(t.round() as usize).clamp(0, feature_length - 1)])
        .collect();

    let mgc_render = utils::interpolate_frames(&features.mgc, &t_render);
    let bap_render = utils::interpolate_frames(&features.bap, &t_render);

    let parsed_flags = flags::parse_flags(&req.flags);

    let mut sp_render = rsworld::decode_spectral_envelope(
        &mgc_render,
        render_length as i32,
        sample_rate as i32,
        consts::FFT_SIZE,
    );
    let ap_render =
        rsworld::decode_aperiodicity(&bap_render, render_length as i32, sample_rate as i32);

    let g_factor = if parsed_flags.g != 0.0 {
        2.0_f64.powf(parsed_flags.g / 100.0)
    } else {
        1.0
    };
    let f_factor = if parsed_flags.f != 0.0 {
        2.0_f64.powf(parsed_flags.f / 12.0)
    } else {
        1.0
    };
    let total_factor = g_factor * f_factor;

    let target_midi = utils::note_to_midi(&req.tone) as f64 + (parsed_flags.t / 100.0);
    let target_base_f0 = utils::midi_to_hz(target_midi);

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

    sp_render.par_iter_mut().for_each(|sp| {
        if (total_factor - 1.0).abs() > 0.001 {
            synthesis::warp_spectrum(sp, sample_rate as f64, total_factor);
        }
        for (d, s) in sp.iter_mut().enumerate() {
            if do_tilt {
                let freq = (d as f64 / fft_size_half) * nyquist;
                if freq > 3500.0 {
                    let freq_scale = (freq - 3500.0) / 4000.0;
                    let reduction_factor = 1.0 + tilt_intensity * 2.0 * freq_scale.powi(2);
                    *s /= reduction_factor;
                }
            }
            *s = s.max(1e-16); // NaN 방지
        }
    });

    let h_factor = if parsed_flags.h > 0.0 {
        (parsed_flags.h.clamp(0.0, 100.0) / 100.0).powi(2)
    } else {
        0.0
    };
    let c_factor = if parsed_flags.c > 0.0 {
        parsed_flags.c.clamp(0.0, 100.0) / 100.0
    } else {
        0.0
    };
    let breathiness_factor = (parsed_flags.b.clamp(0.0, 100.0) - 50.0) / 50.0;
    let b_scale = parsed_flags.b.clamp(0.0, 100.0) / 50.0;

    let onset_fadein_frames = if scaled_cons_sec > 0.0 {
        ((0.050_f64).min(scaled_cons_sec * 0.25) * fps).round() as usize
    } else {
        0
    };

    let mut ap_final = ap_render;
    ap_final.par_iter_mut().enumerate().for_each(|(i, frame)| {
        let is_voiced = vuv_render.get(i).copied().unwrap_or(false);

        let onset_breath_factor = if i < onset_fadein_frames {
            let progress = i as f64 / onset_fadein_frames as f64;
            1.0 - (1.0 - (progress * std::f64::consts::PI).cos()) * 0.5
        } else {
            0.0
        };

        for a in frame.iter_mut() {
            if is_voiced {
                if h_factor > 0.0 {
                    *a *= 1.0 - h_factor;
                }
            } else if c_factor > 0.0 {
                *a *= 1.0 - c_factor;
            }

            if breathiness_factor > 0.0 {
                *a += (1.0 - *a) * breathiness_factor;
            } else if breathiness_factor < 0.0 {
                *a *= b_scale;
            }

            if onset_breath_factor > 0.0 {
                *a += (1.0 - *a) * onset_breath_factor;
            }

            *a = a.clamp(0.0, 1.0);
        }
    });

    let pitchbend_semitones = utils::parse_pitchbend_to_semitones(&req.pitchbend);
    let pps = 8.0 * req.tempo as f64 / 5.0;
    let pitchbend_interp = utils::CubicSplineInterpolator::new(&pitchbend_semitones);

    let modulation = parsed_flags.m / 100.0;

    let f0_render: Vec<f64> = (0..render_length)
        .into_par_iter()
        .map(|i| {
            if vuv_render[i] {
                let t = (i as f64) / fps;
                let pb_idx = t * pps;
                let pb = pitchbend_interp.sample(pb_idx);
                let f0_mod = f0_off_render[i] * modulation;
                utils::midi_to_hz(target_midi + pb + f0_mod)
            } else {
                0.0
            }
        })
        .collect();

    let volume = parsed_flags.a.clamp(0.0, 200.0) / 100.0;

    let mut syn = synthesis::synthesize(
        &f0_render,
        &mut sp_render,
        &mut ap_final,
        sample_rate,
        frame_period,
    );

    let max_amp_orig = syn
        .par_iter()
        .map(|&x| x.abs())
        .reduce(|| 0.0_f64, f64::max);

    if max_amp_orig > 0.0 {
        let d_factor = parsed_flags.d.clamp(0.0, 100.0) / 100.0;
        let d_enabled = d_factor > 0.0;
        let threshold = 1.0 - d_factor * 0.8;
        let ratio = 1.0 + d_factor * 3.0;

        let peak_after_d = if d_enabled {
            max_amp_orig * (threshold + (1.0 - threshold) / ratio)
        } else {
            max_amp_orig
        };

        let peak_after_vol = peak_after_d * volume;
        let target_peak = if parsed_flags.p > 0.0 && parsed_flags.p < 100.0 {
            parsed_flags.p / 100.0
        } else {
            0.99
        };

        let final_scale = if peak_after_vol > target_peak {
            target_peak / peak_after_d
        } else {
            volume
        };

        // If we only have a simple scale of 1.0 and no D-flag, we can skip the loop
        if d_enabled || (final_scale - 1.0).abs() > 0.001 {
            syn.par_iter_mut().for_each(|s| {
                if d_enabled {
                    let abs_s = s.abs() / max_amp_orig;
                    if abs_s > threshold {
                        let over = abs_s - threshold;
                        *s = s.signum() * (threshold + over / ratio) * max_amp_orig;
                    }
                }
                *s *= final_scale;
            });
        }
    }

    write_audio(output_path, &syn, sample_rate)?;
    tracing::info!(
        "Synthesis completed for {:?} in {:?}",
        req.input_file,
        start_synthesis.elapsed()
    );
    Ok(())
}
