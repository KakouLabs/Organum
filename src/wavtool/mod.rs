use anyhow::Result;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

pub mod audio;
pub mod types;

use crate::utils::XorShift32;
pub use audio::{cubic_interpolate, read_wav_samples};
pub use types::{AudioPart, EnvPoint, WavtoolRequest};

#[cfg(test)]
fn analyze_i16_samples(samples: &[i16]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }

    let mut sq_sum = 0.0_f64;
    let mut peak = 0.0_f32;
    for &s in samples {
        let v = s as f32 / 32768.0;
        sq_sum += (v as f64) * (v as f64);
        peak = peak.max(v.abs());
    }

    let rms = (sq_sum / samples.len() as f64).sqrt() as f32;
    (rms, peak)
}

pub fn concatenate(req: &WavtoolRequest) -> Result<()> {
    let start_total = Instant::now();
    let config = crate::config::global_config();
    let sample_rate = config.sample_rate;

    tracing::info!(
        "Wavtool request: {} parts -> {}",
        req.parts.len(),
        req.output_file
    );
    let output_path = Path::new(&req.output_file);

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut canvas: Vec<f32> = Vec::new();

    if output_path.exists() {
        if let Ok((samples, _sr)) = read_wav_samples(output_path) {
            canvas = samples;
        }
    }
    tracing::debug!("Loaded canvas: {} samples", canvas.len());

    for (idx, part) in req.parts.iter().enumerate() {
        let part_start = Instant::now();
        tracing::info!(
            "Part {}: path='{}', len_ms={}, offset_ms={}, skip_ms={}",
            idx,
            part.file_path,
            part.length_ms,
            part.offset_ms,
            part.skip_ms
        );

        let path = Path::new(&part.file_path);
        if !path.exists() {
            tracing::error!("Part {} file missing: {:?}", idx, path);
            continue;
        }

        let (src_samples, src_rate) = match read_wav_samples(path) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to read part {}: {}", idx, e);
                continue;
            }
        };

        if src_samples.is_empty() {
            tracing::warn!("Part {} source has 0 samples", idx);
            continue;
        }

        let dest_start = (part.offset_ms / 1000.0 * sample_rate as f32) as usize;
        let fade_in_samples =
            (part.fade_in_ms.unwrap_or(5.0) / 1000.0 * sample_rate as f32) as usize;
        let fade_out_samples =
            (part.fade_out_ms.unwrap_or(35.0) / 1000.0 * sample_rate as f32) as usize;
        let volume = part.volume.unwrap_or(1.0);

        let output_len = (part.length_ms / 1000.0 * sample_rate as f32) as usize;

        let sr_ratio = src_rate as f32 / sample_rate as f32;
        let skip_samples = part.skip_ms / 1000.0 * src_rate as f32;

        let available_src_f32 = (src_samples.len() as f32 - skip_samples) / sr_ratio;
        let available_src = if available_src_f32 < 0.0 {
            0
        } else {
            available_src_f32 as usize
        };

        let mix_len = if output_len == 0 {
            tracing::info!(
                "Part {} requested length is 0, falling back to source length: {} samples",
                idx,
                available_src
            );
            available_src
        } else {
            output_len
        };

        let write_end = dest_start + mix_len;
        if canvas.len() < write_end {
            canvas.resize(write_end, 0.0);
        }

        let src_len_f32 = src_samples.len() as f32;
        let env_ref = part.envelope.as_ref();
        let env_len = env_ref.map_or(0, |e| e.len());
        let mut cur_env_idx = 0;

        let step_ms = 1000.0 / sample_rate as f32;
        let mut time_ms = 0.0;

        let last_sample = *src_samples.last().unwrap_or(&0.0);

        if let Some(env) = env_ref.filter(|env| !env.is_empty()) {
            for i in 0..mix_len {
                let dest_idx = dest_start + i;
                let src_idx = skip_samples + (i as f32 * sr_ratio);
                let val = if src_idx >= 0.0 && src_idx < src_len_f32 - 1.0 {
                    cubic_interpolate(&src_samples, src_idx) * volume
                } else if src_idx >= src_len_f32 - 1.0 {
                    let fade_progress = (src_idx - (src_len_f32 - 1.0)) / 100.0;
                    last_sample * (1.0 - fade_progress.min(1.0)) * volume
                } else {
                    0.0
                };

                let gain = if time_ms <= env[0].time_ms {
                    env[0].volume
                } else if time_ms >= env[env_len - 1].time_ms {
                    env[env_len - 1].volume
                } else {
                    while cur_env_idx < env_len - 2 && time_ms > env[cur_env_idx + 1].time_ms {
                        cur_env_idx += 1;
                    }
                    let e1 = &env[cur_env_idx];
                    let e2 = &env[cur_env_idx + 1];
                    let dt = (e2.time_ms - e1.time_ms).max(0.0001);
                    let t = (time_ms - e1.time_ms) / dt;
                    e1.volume * (1.0 - t) + e2.volume * t
                };

                canvas[dest_idx] += val * gain;
                time_ms += step_ms;
            }
        } else {
            let fade_out_start = mix_len.saturating_sub(fade_out_samples);
            for i in 0..mix_len {
                let dest_idx = dest_start + i;
                let src_idx = skip_samples + (i as f32 * sr_ratio);
                let val = if src_idx >= 0.0 && src_idx < src_len_f32 - 1.0 {
                    cubic_interpolate(&src_samples, src_idx) * volume
                } else if src_idx >= src_len_f32 - 1.0 {
                    let fade_progress = (src_idx - (src_len_f32 - 1.0)) / 100.0;
                    last_sample * (1.0 - fade_progress.min(1.0)) * volume
                } else {
                    0.0
                };

                let mut gain = 1.0;
                if fade_in_samples > 0 && i < fade_in_samples {
                    gain *= i as f32 / fade_in_samples as f32;
                }
                if fade_out_samples > 0 && mix_len > fade_out_samples && i >= fade_out_start {
                    gain *= (mix_len - i) as f32 / fade_out_samples as f32;
                }

                canvas[dest_idx] += val * gain;
            }
        }

        tracing::debug!("Part {} mixed in {:?}", idx, part_start.elapsed());
    }

    let file = std::fs::File::create(output_path)?;
    let buf_writer = BufWriter::with_capacity(256 * 1024, file);
    let mut writer = hound::WavWriter::new(buf_writer, spec)?;

    let max_amp = canvas.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));

    let threshold = config.compressor_threshold;
    let limit = config.compressor_limit;

    let mut error_accum = 0.0_f32;
    let mut prng = XorShift32::new(0x12345678);
    const WRITE_CHUNK_SAMPLES: usize = 8192;
    for chunk in canvas.chunks(WRITE_CHUNK_SAMPLES) {
        let mut sample_writer = writer.get_i16_writer(chunk.len() as u32);
        for &s in chunk {
            let abs_s = s.abs();
            let sign = s.signum();

            let compressed = if abs_s <= threshold {
                s
            } else if abs_s <= max_amp && max_amp > threshold {
                let ratio = (abs_s - threshold) / (max_amp - threshold + 0.001);
                let target_range = limit - threshold;
                sign * (threshold + ratio.sqrt() * target_range)
            } else {
                sign * limit
            };

            let scaled = compressed * 32767.0 + error_accum;

            let r1 = prng.next_f32();
            let r2 = prng.next_f32();

            let dither = r1 + r2;

            let q = (scaled + dither).round().clamp(-32768.0, 32767.0) as i16;
            error_accum = scaled - q as f32;
            sample_writer.write_sample(q);
        }
        sample_writer.flush()?;
    }
    writer.finalize()?;

    tracing::info!(
        "Wavtool complete. Output: {} samples, MaxAmp: {:.4}, Total: {:?}",
        canvas.len(),
        max_amp,
        start_total.elapsed()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn analyze_i16_samples_basic_stats() {
        let one_quarter = 8192i16;
        let half = 16384i16;
        let samples = vec![one_quarter, -one_quarter, half, -half];

        let (rms, peak) = analyze_i16_samples(&samples);
        assert!((peak - 0.5).abs() < 1e-4);
        assert!((rms - 0.3952847).abs() < 1e-4);
    }

    #[test]
    fn analyze_i16_samples_empty() {
        let (rms, peak) = analyze_i16_samples(&[]);
        assert_eq!(rms, 0.0);
        assert_eq!(peak, 0.0);
    }

    fn estimate_f0_hz(samples: &[i16], sample_rate: u32) -> f32 {
        if samples.len() < 2 {
            return 0.0;
        }
        let mut crossings = 0usize;
        for w in samples.windows(2) {
            if w[0] <= 0 && w[1] > 0 {
                crossings += 1;
            }
        }
        let duration_sec = samples.len() as f32 / sample_rate as f32;
        if duration_sec <= 0.0 {
            0.0
        } else {
            crossings as f32 / duration_sec
        }
    }

    #[test]
    fn concatenate_regression_single_tone_stats() -> Result<()> {
        let tmp = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let in_path = tmp.join(format!("organum_wavtool_in_{}.wav", nonce));
        let out_path = tmp.join(format!("organum_wavtool_out_{}.wav", nonce));

        let sr = 44100u32;
        let amp = 0.5f32;
        let f0 = 440.0f32;
        let frames = (0.2 * sr as f32) as usize;

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: sr,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        {
            let mut writer = hound::WavWriter::create(&in_path, spec)?;
            for i in 0..frames {
                let t = i as f32 / sr as f32;
                let s = (2.0 * PI * f0 * t).sin() * amp;
                let q = (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                writer.write_sample(q)?;
            }
            writer.finalize()?;
        }

        let req = WavtoolRequest {
            output_file: out_path.to_string_lossy().to_string(),
            parts: vec![AudioPart {
                file_path: in_path.to_string_lossy().to_string(),
                offset_ms: 0.0,
                skip_ms: 0.0,
                length_ms: 200.0,
                fade_in_ms: Some(0.0),
                fade_out_ms: Some(0.0),
                volume: Some(1.0),
                envelope: None,
            }],
        };

        concatenate(&req)?;

        let mut reader = hound::WavReader::open(&out_path)?;
        let out: Vec<i16> = reader
            .samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()?;

        assert!((out.len() as i32 - frames as i32).abs() <= 2);
        let (rms, peak) = analyze_i16_samples(&out);
        assert!((rms - (amp / 2.0_f32.sqrt())).abs() < 0.03);
        assert!((peak - amp).abs() < 0.03);

        let est_f0 = estimate_f0_hz(&out, sr);
        assert!((est_f0 - f0).abs() < 5.0);

        let _ = fs::remove_file(in_path);
        let _ = fs::remove_file(out_path);

        Ok(())
    }
}
