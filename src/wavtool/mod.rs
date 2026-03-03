use anyhow::Result;
use std::io::BufWriter;
use std::path::Path;

pub mod audio;
pub mod types;

use crate::utils::XorShift32;
pub use audio::{cubic_interpolate, read_wav_samples};
pub use types::{AudioPart, EnvPoint, WavtoolRequest};

pub fn concatenate(req: &WavtoolRequest) -> Result<()> {
    let config = crate::config::load_config();
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

        for i in 0..mix_len {
            let dest_idx = dest_start + i;

            let src_idx = skip_samples + (i as f32 * sr_ratio);
            let val = if src_idx >= 0.0 && src_idx < src_len_f32 - 1.0 {
                cubic_interpolate(&src_samples, src_idx) * volume
            } else if src_idx >= src_len_f32 - 1.0 {
                let last_val = *src_samples.last().unwrap_or(&0.0);
                let fade_progress = (src_idx - (src_len_f32 - 1.0)) / 100.0;
                last_val * (1.0 - fade_progress.min(1.0)) * volume
            } else {
                0.0
            };

            let mut gain = 1.0;

            if let Some(env) = env_ref {
                if env_len > 0 {
                    if time_ms <= env[0].time_ms {
                        gain *= env[0].volume;
                    } else if time_ms >= env[env_len - 1].time_ms {
                        gain *= env[env_len - 1].volume;
                    } else {
                        while cur_env_idx < env_len - 2 && time_ms > env[cur_env_idx + 1].time_ms {
                            cur_env_idx += 1;
                        }
                        let e1 = &env[cur_env_idx];
                        let e2 = &env[cur_env_idx + 1];

                        let dt = (e2.time_ms - e1.time_ms).max(0.0001);
                        let t = (time_ms - e1.time_ms) / dt;
                        gain *= e1.volume * (1.0 - t) + e2.volume * t;
                    }
                }
            } else {
                if fade_in_samples > 0 && i < fade_in_samples {
                    let t = i as f32 / fade_in_samples as f32;
                    gain *= t;
                }
                if fade_out_samples > 0
                    && mix_len > fade_out_samples
                    && i >= mix_len - fade_out_samples
                {
                    let remaining = mix_len - i;
                    let t = remaining as f32 / fade_out_samples as f32;
                    gain *= t;
                }
            }

            canvas[dest_idx] += val * gain;
            time_ms += step_ms;
        }
    }

    let file = std::fs::File::create(output_path)?;
    let buf_writer = BufWriter::with_capacity(256 * 1024, file);
    let mut writer = hound::WavWriter::new(buf_writer, spec)?;

    let max_amp = canvas.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));

    let threshold = config.compressor_threshold;
    let limit = config.compressor_limit;

    let mut error_accum = 0.0_f32;
    let mut prng = XorShift32::new(0x12345678);
    for &s in &canvas {
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

        // TPDF dither
        let r1 = prng.next_f32();
        let r2 = prng.next_f32();

        let dither = r1 + r2;

        let q = (scaled + dither).round().clamp(-32768.0, 32767.0) as i16;
        error_accum = scaled - q as f32;
        writer.write_sample(q)?;
    }
    writer.finalize()?;

    tracing::info!(
        "Wavtool complete. Output: {} samples, MaxAmp: {:.4}",
        canvas.len(),
        max_amp
    );
    Ok(())
}
