use crate::resampler::{
    consts,
    types::{FeatureCacheV4, FeatureCacheV4Delta, FeatureCacheV4Quantized, WorldFeatures},
    utils::calculate_base_f0,
};
use anyhow::Result;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

const CACHE_MAGIC: [u8; 4] = *b"ORGN";
const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Quantized,
    DeltaQuantized,
    Unknown,
}

/// Returns true when cache file has the current magic/version header.
pub fn is_feature_cache_compatible(path: &Path) -> bool {
    let mut f = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };

    let mut magic = [0u8; 4];
    if f.read_exact(&mut magic).is_err() || magic != CACHE_MAGIC {
        return false;
    }

    let mut version_bytes = [0u8; 4];
    if f.read_exact(&mut version_bytes).is_err() {
        return false;
    }

    u32::from_le_bytes(version_bytes) == CACHE_VERSION
}

pub fn generate_features(
    audio: &[f64],
    sample_rate: u32,
    frame_period: f64,
) -> Result<WorldFeatures> {
    use rsworld::{cheaptrick, code_aperiodicity, code_spectral_envelope, d4c, dio, stonemask};
    use rsworld_sys::{CheapTrickOption, D4COption, DioOption};

    let dio_opts = DioOption {
        f0_floor: consts::F0_FLOOR,
        f0_ceil: consts::F0_CEIL,
        frame_period,
        channels_in_octave: 2.0,
        speed: 1,
        allowed_range: 0.1,
    };

    let mut cheaptrick_opts = CheapTrickOption {
        q1: consts::SPEC_Q1,
        f0_floor: consts::F0_FLOOR,
        fft_size: consts::FFT_SIZE,
    };
    let d4c_opts = D4COption {
        threshold: consts::D4C_THRESHOLD,
    };

    let audio_vec: Vec<f64> = audio.to_vec();

    let (t, f0_rough) = dio(&audio_vec, sample_rate as i32, &dio_opts);
    let f0 = stonemask(&audio_vec, sample_rate as i32, &t, &f0_rough);

    let sp = cheaptrick(
        &audio_vec,
        sample_rate as i32,
        &t,
        &f0,
        &mut cheaptrick_opts,
    );

    let mut ap = d4c(&audio_vec, sample_rate as i32, &t, &f0, &d4c_opts);

    for ap_frame in ap.iter_mut() {
        for a in ap_frame.iter_mut() {
            if a.is_nan() {
                *a = 0.0;
            }
        }
    }

    let base_f0 = calculate_base_f0(&f0);
    let mgc = code_spectral_envelope(
        &sp,
        f0.len() as i32,
        sample_rate as i32,
        consts::FFT_SIZE,
        consts::MGC_DIMS,
    );
    let bap = code_aperiodicity(&ap, f0.len() as i32, sample_rate as i32);

    Ok(WorldFeatures {
        base_f0,
        f0,
        mgc,
        bap,
    })
}

pub fn read_features(path: &Path) -> Result<WorldFeatures> {
    let mut f = File::open(path)?;

    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)?;
    if magic != CACHE_MAGIC {
        anyhow::bail!(
            "Invalid cache magic (expected {:?}, got {:?})",
            CACHE_MAGIC,
            magic
        );
    }

    let mut version_bytes = [0u8; 4];
    f.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    if version != CACHE_VERSION {
        anyhow::bail!(
            "Cache version mismatch (expected {}, got {})",
            CACHE_VERSION,
            version
        );
    }

    let mut decoder = zstd::stream::Decoder::new(f)?;
    let cached: FeatureCacheV4 = bincode::deserialize_from(&mut decoder)?;
    cached.try_into()
}

pub fn write_features(path: &Path, features: &WorldFeatures, compression_level: i32) -> Result<()> {
    let mut f = File::create(path)?;
    f.write_all(&CACHE_MAGIC)?;
    f.write_all(&CACHE_VERSION.to_le_bytes())?;

    let q_payload = FeatureCacheV4::Quantized(FeatureCacheV4Quantized::from(features));
    let d_payload = FeatureCacheV4::DeltaQuantized(FeatureCacheV4Delta::from(features));

    match estimate_best_strategy(features) {
        Strategy::Quantized => {
            let q_bytes = compress_payload(&q_payload, compression_level)?;
            f.write_all(&q_bytes)?;
            tracing::debug!("Cache strategy=quantized(heuristic) q={}B", q_bytes.len());
        }
        Strategy::DeltaQuantized => {
            let d_bytes = compress_payload(&d_payload, compression_level)?;
            f.write_all(&d_bytes)?;
            tracing::debug!(
                "Cache strategy=delta-quantized(heuristic) d={}B",
                d_bytes.len()
            );
        }
        Strategy::Unknown => {
            // Ambiguous signal statistics: compare both and keep the smaller payload.
            let q_bytes = compress_payload(&q_payload, compression_level)?;
            let d_bytes = compress_payload(&d_payload, compression_level)?;
            if q_bytes.len() <= d_bytes.len() {
                f.write_all(&q_bytes)?;
                tracing::debug!(
                    "Cache strategy=quantized(fallback) q={}B d={}B",
                    q_bytes.len(),
                    d_bytes.len()
                );
            } else {
                f.write_all(&d_bytes)?;
                tracing::debug!(
                    "Cache strategy=delta-quantized(fallback) q={}B d={}B",
                    q_bytes.len(),
                    d_bytes.len()
                );
            }
        }
    }

    Ok(())
}

fn compress_payload(payload: &FeatureCacheV4, compression_level: i32) -> Result<Vec<u8>> {
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), compression_level)?;
    bincode::serialize_into(&mut encoder, payload)?;
    Ok(encoder.finish()?)
}

fn estimate_best_strategy(features: &WorldFeatures) -> Strategy {
    let (mean_abs, mean_delta_abs) = spectral_activity_metrics(features);
    if mean_abs <= 1e-8 {
        return Strategy::DeltaQuantized;
    }

    // Lower score means smoother frame-to-frame changes, where delta coding is usually better.
    let roughness = mean_delta_abs / mean_abs;

    if roughness <= 0.45 {
        Strategy::DeltaQuantized
    } else if roughness >= 0.75 {
        Strategy::Quantized
    } else {
        Strategy::Unknown
    }
}

fn spectral_activity_metrics(features: &WorldFeatures) -> (f32, f32) {
    let mut abs_sum = 0.0_f64;
    let mut abs_count: usize = 0;
    let mut delta_sum = 0.0_f64;
    let mut delta_count: usize = 0;

    for row in &features.mgc {
        accumulate_row_metrics(row, &mut abs_sum, &mut abs_count, &mut delta_sum, &mut delta_count);
    }
    for row in &features.bap {
        accumulate_row_metrics(row, &mut abs_sum, &mut abs_count, &mut delta_sum, &mut delta_count);
    }

    let mean_abs = if abs_count > 0 {
        (abs_sum / abs_count as f64) as f32
    } else {
        0.0
    };
    let mean_delta_abs = if delta_count > 0 {
        (delta_sum / delta_count as f64) as f32
    } else {
        0.0
    };

    (mean_abs, mean_delta_abs)
}

fn accumulate_row_metrics(
    row: &[f64],
    abs_sum: &mut f64,
    abs_count: &mut usize,
    delta_sum: &mut f64,
    delta_count: &mut usize,
) {
    if row.is_empty() {
        return;
    }

    for &v in row {
        *abs_sum += v.abs();
        *abs_count += 1;
    }

    for w in row.windows(2) {
        *delta_sum += (w[1] - w[0]).abs();
        *delta_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resampler::types::WorldFeatures;
    use std::env;

    fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn test_features_roundtrip() -> Result<()> {
        let features = WorldFeatures {
            base_f0: 440.0,
            f0: vec![440.0, 442.0, 445.0],
            mgc: vec![
                vec![0.1, 0.2, 0.3],
                vec![0.15, 0.25, 0.35],
                vec![0.2, 0.3, 0.4],
            ],
            bap: vec![vec![-0.1, -0.2], vec![-0.15, -0.25], vec![-0.2, -0.3]],
        };

        let mut temp_path = env::temp_dir();
        temp_path.push("organum_test_features.ogc");

        // write
        write_features(&temp_path, &features, 3)?;

        // read
        let read_back = read_features(&temp_path)?;

        assert!(approx_eq(read_back.base_f0, features.base_f0, 1e-4));
        for (a, b) in read_back.f0.iter().zip(features.f0.iter()) {
            assert!(approx_eq(*a, *b, 1e-3));
        }
        for (row_a, row_b) in read_back.mgc.iter().zip(features.mgc.iter()) {
            for (a, b) in row_a.iter().zip(row_b.iter()) {
                assert!(approx_eq(*a, *b, 1e-3));
            }
        }
        for (row_a, row_b) in read_back.bap.iter().zip(features.bap.iter()) {
            for (a, b) in row_a.iter().zip(row_b.iter()) {
                assert!(approx_eq(*a, *b, 1e-3));
            }
        }
        assert!(is_feature_cache_compatible(&temp_path));

        // clean up
        let _ = std::fs::remove_file(temp_path);

        Ok(())
    }
}
