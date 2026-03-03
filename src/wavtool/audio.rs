use anyhow::Result;
use std::path::Path;

#[inline(always)]
pub fn cubic_interpolate(samples: &[f32], idx: f32) -> f32 {
    crate::utils::cubic_interpolate_f32(samples, idx)
}

pub fn read_wav_samples(path: &Path) -> Result<(Vec<f32>, u32)> {
    let (samples_f64, sr) = crate::utils::decode_wav_samples(path)?;
    let samples_f32: Vec<f32> = samples_f64.into_iter().map(|s| s as f32).collect();
    Ok((samples_f32, sr))
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
