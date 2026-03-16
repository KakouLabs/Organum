use crate::resampler::{
    common::consts,
    common::utils::calculate_base_f0,
    types::{
        compute_cache_key, decode_v5_payload, encode_v5_payload, v5_payload_upper_bound,
        CacheV5Header, WorldFeatures, WorldFeaturesOwned, CACHE_V5_FORMAT_VERSION,
        CACHE_V5_HEADER_SIZE, CACHE_V5_MAGIC,
    },
};
use anyhow::Result;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

#[inline]
fn frame_period_micros(config: &crate::config::OrganumConfig) -> u32 {
    (config.frame_period * 1000.0).round() as u32
}

#[inline]
fn expected_cache_key(config: &crate::config::OrganumConfig) -> u64 {
    compute_cache_key(config.sample_rate, frame_period_micros(config))
}

fn read_v5_header<R: Read>(reader: &mut R) -> Result<CacheV5Header> {
    let mut bytes = [0u8; CACHE_V5_HEADER_SIZE];
    reader.read_exact(&mut bytes)?;
    Ok(CacheV5Header::from_bytes(&bytes))
}

fn validate_header(header: &CacheV5Header, config: &crate::config::OrganumConfig) -> Result<()> {
    let magic = header.magic;
    let format_version = header.format_version;
    let cache_key = header.cache_key;
    let sample_rate = header.sample_rate;
    let period_micros = header.frame_period_micros;

    if magic != CACHE_V5_MAGIC {
        anyhow::bail!("Invalid cache magic");
    }
    if format_version != CACHE_V5_FORMAT_VERSION {
        anyhow::bail!(
            "Cache format version mismatch (expected {}, got {})",
            CACHE_V5_FORMAT_VERSION,
            format_version
        );
    }
    if cache_key != expected_cache_key(config) {
        anyhow::bail!("Cache key mismatch");
    }
    if sample_rate != config.sample_rate {
        anyhow::bail!(
            "Cache sample_rate mismatch (expected {}, got {})",
            config.sample_rate,
            sample_rate
        );
    }
    if period_micros != frame_period_micros(config) {
        anyhow::bail!(
            "Cache frame_period mismatch (expected {}us, got {}us)",
            frame_period_micros(config),
            period_micros
        );
    }
    Ok(())
}

pub fn is_feature_cache_compatible(path: &Path, config: &crate::config::OrganumConfig) -> bool {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let header = match read_v5_header(&mut f) {
        Ok(h) => h,
        Err(_) => return false,
    };
    validate_header(&header, config).is_ok()
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

    let audio_vec = audio.to_vec();
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

pub fn read_features(path: &Path, config: &crate::config::OrganumConfig) -> Result<WorldFeatures> {
    let mut reader = BufReader::with_capacity(64 * 1024, File::open(path)?);
    let header = read_v5_header(&mut reader)?;
    validate_header(&header, config)?;

    let mut decoder = zstd::stream::Decoder::new(reader)?;
    let payload_size = header.payload_size as usize;
    let frame_count = header.frame_count as usize;
    let mgc_dims = header.mgc_dims as usize;
    let bap_dims = header.bap_dims as usize;

    let mut payload = Vec::with_capacity(payload_size);
    decoder.read_to_end(&mut payload)?;

    if payload.len() != payload_size {
        anyhow::bail!(
            "Cache payload size mismatch (expected {}, got {})",
            payload_size,
            payload.len()
        );
    }

    let owned = decode_v5_payload(&payload, frame_count, mgc_dims, bap_dims)?;
    Ok(owned.to_world_features())
}

pub fn write_features(
    path: &Path,
    features: &WorldFeatures,
    compression_level: i32,
    config: &crate::config::OrganumConfig,
) -> Result<()> {
    let owned = WorldFeaturesOwned::from_world_features(features);
    let payload = encode_v5_payload(&owned);

    let upper_bound = v5_payload_upper_bound(owned.f0.len(), owned.mgc.cols, owned.bap.cols);
    if payload.len() > upper_bound {
        anyhow::bail!(
            "Internal payload size mismatch (upper bound {}, got {})",
            upper_bound,
            payload.len()
        );
    }

    let header = CacheV5Header {
        magic: CACHE_V5_MAGIC,
        format_version: CACHE_V5_FORMAT_VERSION,
        cache_key: expected_cache_key(config),
        sample_rate: config.sample_rate,
        frame_period_micros: frame_period_micros(config),
        frame_count: owned.f0.len() as u32,
        mgc_dims: owned.mgc.cols as u16,
        bap_dims: owned.bap.cols as u16,
        payload_size: payload.len() as u32,
        _reserved: 0,
    };

    let tmp_path = path.with_extension("ogc.tmp");
    let write_result = (|| -> Result<()> {
        let mut file = BufWriter::with_capacity(64 * 1024, File::create(&tmp_path)?);
        file.write_all(&header.to_bytes())?;

        let mut encoder = zstd::stream::Encoder::new(file, compression_level)?;
        encoder.write_all(&payload)?;
        let mut file = encoder.finish()?;
        file.flush()?;
        Ok(())
    })();

    match write_result {
        Ok(()) => {
            std::fs::rename(&tmp_path, path)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let config = crate::config::OrganumConfig::default();

        write_features(&temp_path, &features, 3, &config)?;
        let read_back = read_features(&temp_path, &config)?;

        assert!(approx_eq(read_back.base_f0, features.base_f0, 0.1));
        for (a, b) in read_back.f0.iter().zip(features.f0.iter()) {
            assert!(approx_eq(*a, *b, 0.5));
        }
        for (row_a, row_b) in read_back.mgc.iter().zip(features.mgc.iter()) {
            for (a, b) in row_a.iter().zip(row_b.iter()) {
                assert!(approx_eq(*a, *b, 0.01));
            }
        }
        for (row_a, row_b) in read_back.bap.iter().zip(features.bap.iter()) {
            for (a, b) in row_a.iter().zip(row_b.iter()) {
                assert!(approx_eq(*a, *b, 0.01));
            }
        }
        assert!(is_feature_cache_compatible(&temp_path, &config));

        let mut incompatible = config.clone();
        incompatible.sample_rate = 48000;
        assert!(!is_feature_cache_compatible(&temp_path, &incompatible));

        let _ = std::fs::remove_file(temp_path);
        Ok(())
    }
}
