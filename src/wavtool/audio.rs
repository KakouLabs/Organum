use anyhow::{Context, Result};
use std::path::Path;

#[inline(always)]
pub fn cubic_interpolate(samples: &[f32], idx: f32) -> f32 {
    crate::utils::cubic_interpolate_f32(samples, idx)
}

pub fn read_wav_samples(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut reader =
        hound::WavReader::open(path).context(format!("Failed to open WAV: {:?}", path))?;
    let spec = reader.spec();
    let max_val: f32 = match spec.bits_per_sample {
        8 => 128.0,
        16 => 32768.0,
        24 => 8388608.0,
        32 => 2147483648.0,
        _ => 32768.0,
    };
    let channels = spec.channels as usize;
    let total_samples = reader.len() as usize;
    let estimated_frames = total_samples / channels.max(1);

    let mut mono: Vec<f32> = Vec::with_capacity(estimated_frames);

    if channels <= 1 {
        for s in reader.samples::<i32>() {
            let sample = s.unwrap_or(0);
            mono.push(sample as f32 / max_val);
        }
    } else {
        let inv_ch = 1.0 / (channels as f32 * max_val);
        let mut ch_sum: f32 = 0.0;
        let mut ch_idx: usize = 0;

        for s in reader.samples::<i32>() {
            let sample = s.unwrap_or(0);
            ch_sum += sample as f32;
            ch_idx += 1;

            if ch_idx == channels {
                mono.push(ch_sum * inv_ch);
                ch_sum = 0.0;
                ch_idx = 0;
            }
        }
    }

    Ok((mono, spec.sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cubic_interpolate() {
        let samples = vec![0.0, 10.0, 20.0, 30.0];
        // At exact indices
        assert_eq!(cubic_interpolate(&samples, 1.0), 10.0);
        assert_eq!(cubic_interpolate(&samples, 2.0), 20.0);

        // Out of bounds / Clamp testing
        assert_eq!(cubic_interpolate(&samples, -1.0), 0.0);
        assert_eq!(cubic_interpolate(&samples, 5.0), 30.0);

        // Single element
        let single = vec![42.0];
        assert_eq!(cubic_interpolate(&single, 0.5), 42.0);

        // Empty
        let empty: Vec<f32> = vec![];
        assert_eq!(cubic_interpolate(&empty, 0.0), 0.0);
    }
}
