use crate::resampler::{consts, types::WorldFeatures, utils::calculate_base_f0};
use anyhow::Result;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

const CACHE_MAGIC: [u8; 4] = *b"ORGN";
const CACHE_VERSION: u32 = 1;

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
    let features: WorldFeatures = bincode::deserialize_from(&mut decoder)?;
    Ok(features)
}

pub fn write_features(path: &Path, features: &WorldFeatures, compression_level: i32) -> Result<()> {
    let mut f = File::create(path)?;
    f.write_all(&CACHE_MAGIC)?;
    f.write_all(&CACHE_VERSION.to_le_bytes())?;

    let mut encoder = zstd::stream::Encoder::new(f, compression_level)?;
    bincode::serialize_into(&mut encoder, features)?;
    encoder.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resampler::types::WorldFeatures;
    use std::env;

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

        assert_eq!(read_back.base_f0, features.base_f0);
        assert_eq!(read_back.f0, features.f0);
        assert_eq!(read_back.mgc, features.mgc);
        assert_eq!(read_back.bap, features.bap);

        // clean up
        let _ = std::fs::remove_file(temp_path);

        Ok(())
    }
}
