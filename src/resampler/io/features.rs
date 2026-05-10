use crate::resampler::common::utils::{calculate_base_f0, compute_file_hash};
use crate::resampler::{
    common::consts,
    types::{
        compute_cache_key, decode_v5_payload, encode_v5_payload_features, v5_payload_upper_bound,
        CacheV5Header, MatrixF64, WorldFeatures, WorldFeaturesOwned, CACHE_V5_FORMAT_VERSION,
        CACHE_V5_HEADER_SIZE, CACHE_V5_MAGIC,
    },
};
use anyhow::Result;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const MAX_CACHE_FRAME_COUNT: usize = 120_000;
const MAX_CACHE_MGC_DIMS: usize = 256;
const MAX_CACHE_BAP_DIMS: usize = 64;
const MAX_CACHE_PAYLOAD_SIZE: usize = 64 * 1024 * 1024;

#[inline]
fn build_native_analysis_config(
    sample_rate: u32,
    frame_period: f32,
) -> world::native::AcousticConfig {
    let mut config = world::native::AcousticConfig::new(sample_rate as i32);
    config.f0_estimation.frame_period = frame_period;
    config.f0_estimation.f0_floor = consts::F0_FLOOR;
    config.f0_estimation.f0_ceil = consts::F0_CEIL;
    config.spectral_analysis.f0_floor = consts::F0_FLOOR;
    config.spectral_analysis.q1 = consts::SPEC_Q1;
    config.aperiodicity_analysis.threshold = consts::D4C_THRESHOLD;
    config
}

#[inline]
fn matrix_f32_to_f64(matrix: world::native::MatrixF32) -> MatrixF64 {
    MatrixF64 {
        rows: matrix.rows(),
        cols: matrix.cols(),
        data: matrix.into_vec().into_iter().map(f64::from).collect(),
    }
}

#[inline]
fn frame_period_micros(config: &crate::config::OrganumConfig) -> u32 {
    (config.frame_period * 1000.0).round() as u32
}

#[inline]
fn expected_cache_key(config: &crate::config::OrganumConfig, source_hash: u64) -> u64 {
    compute_cache_key(config.sample_rate, frame_period_micros(config), source_hash)
}

fn read_v5_header<R: Read>(reader: &mut R) -> Result<CacheV5Header> {
    let mut bytes = [0u8; CACHE_V5_HEADER_SIZE];
    reader.read_exact(&mut bytes)?;
    Ok(CacheV5Header::from_bytes(&bytes))
}

fn validate_header(
    header: &CacheV5Header,
    config: &crate::config::OrganumConfig,
    source_hash: u64,
) -> Result<()> {
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
    if !config.ignore_cache_hash_verification
        && cache_key != expected_cache_key(config, source_hash)
    {
        anyhow::bail!("Cache key mismatch (source file might have changed)");
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

fn validate_payload_shape(
    frame_count: usize,
    mgc_dims: usize,
    bap_dims: usize,
    payload_size: usize,
) -> Result<()> {
    if frame_count > MAX_CACHE_FRAME_COUNT {
        anyhow::bail!(
            "Cache frame_count {} exceeds maximum {}",
            frame_count,
            MAX_CACHE_FRAME_COUNT
        );
    }
    if mgc_dims == 0 || mgc_dims > MAX_CACHE_MGC_DIMS {
        anyhow::bail!(
            "Cache mgc_dims {} is outside allowed range 1..={}",
            mgc_dims,
            MAX_CACHE_MGC_DIMS
        );
    }
    if bap_dims == 0 || bap_dims > MAX_CACHE_BAP_DIMS {
        anyhow::bail!(
            "Cache bap_dims {} is outside allowed range 1..={}",
            bap_dims,
            MAX_CACHE_BAP_DIMS
        );
    }
    if payload_size > MAX_CACHE_PAYLOAD_SIZE {
        anyhow::bail!(
            "Cache payload_size {} exceeds maximum {}",
            payload_size,
            MAX_CACHE_PAYLOAD_SIZE
        );
    }

    let upper_bound = v5_payload_upper_bound(frame_count, mgc_dims, bap_dims);
    if payload_size > upper_bound {
        anyhow::bail!(
            "Cache payload_size {} exceeds shape upper bound {}",
            payload_size,
            upper_bound
        );
    }

    Ok(())
}

fn validate_features_shape_for_write(features: &WorldFeatures) -> Result<()> {
    let frame_count = features.f0.len();
    if features.mgc.rows != frame_count {
        anyhow::bail!(
            "Feature/cache shape mismatch: mgc rows {} != f0 frames {}",
            features.mgc.rows,
            frame_count
        );
    }
    if features.bap.rows != frame_count {
        anyhow::bail!(
            "Feature/cache shape mismatch: bap rows {} != f0 frames {}",
            features.bap.rows,
            frame_count
        );
    }

    if features.mgc.cols == 0 || features.mgc.cols > MAX_CACHE_MGC_DIMS {
        anyhow::bail!(
            "Feature/cache mgc_dims {} is outside allowed range 1..={}",
            features.mgc.cols,
            MAX_CACHE_MGC_DIMS
        );
    }
    if features.bap.cols == 0 || features.bap.cols > MAX_CACHE_BAP_DIMS {
        anyhow::bail!(
            "Feature/cache bap_dims {} is outside allowed range 1..={}",
            features.bap.cols,
            MAX_CACHE_BAP_DIMS
        );
    }
    if frame_count > MAX_CACHE_FRAME_COUNT {
        anyhow::bail!(
            "Feature/cache frame_count {} exceeds maximum {}",
            frame_count,
            MAX_CACHE_FRAME_COUNT
        );
    }

    Ok(())
}

pub fn is_feature_cache_compatible(
    path: &Path,
    source_path: &Path,
    config: &crate::config::OrganumConfig,
) -> bool {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let header = match read_v5_header(&mut f) {
        Ok(h) => h,
        Err(_) => return false,
    };
    let source_hash = compute_file_hash(source_path);
    validate_header(&header, config, source_hash).is_ok()
}

pub fn generate_features(
    audio: Vec<f32>,
    sample_rate: u32,
    frame_period: f32,
) -> Result<WorldFeatures> {
    generate_features_native(audio, sample_rate, frame_period)
}

fn generate_features_native(
    audio: Vec<f32>,
    sample_rate: u32,
    frame_period: f32,
) -> Result<WorldFeatures> {
    use world::native::{code_aperiodicity, code_spectral_envelope, AcousticAnalyzer};

    let sample_rate_i32 = sample_rate as i32;
    let config = build_native_analysis_config(sample_rate, frame_period);
    let mut analyzer = AcousticAnalyzer::with_config(config);

    let features_native = analyzer.extract_features(&audio, sample_rate_i32);

    let base_f0 = calculate_base_f0(&features_native.f0) as f64;

    let mgc_native = code_spectral_envelope(
        &features_native.spectrogram,
        sample_rate_i32,
        consts::MGC_DIMS,
    );
    let bap_native = code_aperiodicity(&features_native.aperiodicity, sample_rate_i32);

    let mgc = matrix_f32_to_f64(mgc_native);
    let bap = matrix_f32_to_f64(bap_native);

    Ok(WorldFeatures {
        base_f0,
        f0: features_native.f0.into_iter().map(f64::from).collect(),
        mgc,
        bap,
    })
}

pub fn read_features(
    path: &Path,
    source_path: &Path,
    config: &crate::config::OrganumConfig,
) -> Result<WorldFeatures> {
    Ok(read_features_owned(path, source_path, config)?.to_world_features())
}

pub fn read_features_owned(
    path: &Path,
    source_path: &Path,
    config: &crate::config::OrganumConfig,
) -> Result<WorldFeaturesOwned> {
    let mut reader = BufReader::with_capacity(64 * 1024, File::open(path)?);
    let header = read_v5_header(&mut reader)?;
    let source_hash = compute_file_hash(source_path);
    validate_header(&header, config, source_hash)?;

    let payload_size = header.payload_size as usize;
    let frame_count = header.frame_count as usize;
    let mgc_dims = header.mgc_dims as usize;
    let bap_dims = header.bap_dims as usize;

    validate_payload_shape(frame_count, mgc_dims, bap_dims, payload_size)?;

    let decoder = zstd::stream::Decoder::new(reader)?;
    let mut payload = Vec::with_capacity(payload_size);
    let read_limit = (payload_size as u64).saturating_add(1);
    decoder.take(read_limit).read_to_end(&mut payload)?;

    if payload.len() != payload_size {
        anyhow::bail!(
            "Cache payload size mismatch (expected {}, got {})",
            payload_size,
            payload.len()
        );
    }

    decode_v5_payload(&payload, frame_count, mgc_dims, bap_dims)
}

pub fn write_features(
    path: &Path,
    source_path: &Path,
    features: &WorldFeatures,
    compression_level: i32,
    config: &crate::config::OrganumConfig,
) -> Result<()> {
    validate_features_shape_for_write(features)?;

    let frame_count = features.f0.len();
    let mgc_dims = features.mgc.cols;
    let bap_dims = features.bap.cols;
    let payload = encode_v5_payload_features(features);

    validate_payload_shape(frame_count, mgc_dims, bap_dims, payload.len())?;

    let upper_bound = v5_payload_upper_bound(frame_count, mgc_dims, bap_dims);
    if payload.len() > upper_bound {
        anyhow::bail!(
            "Internal payload size mismatch (upper bound {}, got {})",
            upper_bound,
            payload.len()
        );
    }

    let frame_count: u32 = frame_count
        .try_into()
        .map_err(|_| anyhow::anyhow!("frame_count does not fit in cache header"))?;
    let mgc_dims: u16 = mgc_dims
        .try_into()
        .map_err(|_| anyhow::anyhow!("mgc_dims does not fit in cache header"))?;
    let bap_dims: u16 = bap_dims
        .try_into()
        .map_err(|_| anyhow::anyhow!("bap_dims does not fit in cache header"))?;
    let payload_size: u32 = payload
        .len()
        .try_into()
        .map_err(|_| anyhow::anyhow!("payload_size does not fit in cache header"))?;

    let source_hash = compute_file_hash(source_path);
    let header = CacheV5Header {
        magic: CACHE_V5_MAGIC,
        format_version: CACHE_V5_FORMAT_VERSION,
        cache_key: expected_cache_key(config, source_hash),
        sample_rate: config.sample_rate,
        frame_period_micros: frame_period_micros(config),
        frame_count,
        mgc_dims,
        bap_dims,
        payload_size,
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
    use crate::resampler::types::MatrixF64;
    use std::env;

    fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    fn test_header(config: &crate::config::OrganumConfig, source_hash: u64) -> CacheV5Header {
        CacheV5Header {
            magic: CACHE_V5_MAGIC,
            format_version: CACHE_V5_FORMAT_VERSION,
            cache_key: expected_cache_key(config, source_hash),
            sample_rate: config.sample_rate,
            frame_period_micros: frame_period_micros(config),
            frame_count: 1,
            mgc_dims: 1,
            bap_dims: 1,
            payload_size: 0,
            _reserved: 0,
        }
    }

    #[test]
    fn test_features_roundtrip() -> Result<()> {
        let features = WorldFeatures {
            base_f0: 440.0,
            f0: vec![440.0, 442.0, 445.0],
            mgc: MatrixF64 {
                rows: 3,
                cols: 3,
                data: vec![0.1, 0.2, 0.3, 0.15, 0.25, 0.35, 0.2, 0.3, 0.4],
            },
            bap: MatrixF64 {
                rows: 3,
                cols: 2,
                data: vec![-0.1, -0.2, -0.15, -0.25, -0.2, -0.3],
            },
        };

        let mut temp_path = env::temp_dir();
        temp_path.push("organum_test_features.ogc");
        let mut source_path = env::temp_dir();
        source_path.push("organum_test_source.wav");
        std::fs::write(&source_path, "fake wav data")?;

        let config = crate::config::OrganumConfig::default();

        write_features(&temp_path, &source_path, &features, 3, &config)?;
        let read_back = read_features(&temp_path, &source_path, &config)?;

        assert!(approx_eq(read_back.base_f0, features.base_f0, 0.1));
        for (a, b) in read_back.f0.iter().zip(features.f0.iter()) {
            assert!(approx_eq(*a, *b, 0.5));
        }
        for (a, b) in read_back.mgc.data.iter().zip(features.mgc.data.iter()) {
            assert!(approx_eq(*a, *b, 0.01));
        }
        for (a, b) in read_back.bap.data.iter().zip(features.bap.data.iter()) {
            assert!(approx_eq(*a, *b, 0.01));
        }
        assert!(is_feature_cache_compatible(
            &temp_path,
            &source_path,
            &config
        ));

        let mut incompatible = config.clone();
        incompatible.sample_rate = 48000;
        assert!(!is_feature_cache_compatible(
            &temp_path,
            &source_path,
            &incompatible
        ));

        let _ = std::fs::remove_file(temp_path);
        let _ = std::fs::remove_file(source_path);
        Ok(())
    }

    #[test]
    fn write_features_rejects_mismatched_matrix_rows() -> Result<()> {
        let features = WorldFeatures {
            base_f0: 440.0,
            f0: vec![440.0, 442.0, 445.0],
            mgc: MatrixF64 {
                rows: 2,
                cols: 2,
                data: vec![0.1, 0.2, 0.3, 0.4],
            },
            bap: MatrixF64 {
                rows: 3,
                cols: 1,
                data: vec![-0.1, -0.2, -0.3],
            },
        };

        let mut temp_path = env::temp_dir();
        temp_path.push("organum_test_mismatched_features.ogc");
        let mut source_path = env::temp_dir();
        source_path.push("organum_test_source_mismatched.wav");
        std::fs::write(&source_path, "fake wav data")?;

        let config = crate::config::OrganumConfig::default();

        let err = match write_features(&temp_path, &source_path, &features, 3, &config) {
            Ok(_) => anyhow::bail!("mismatched cache shape should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("mgc rows"));
        assert!(!temp_path.exists());

        let _ = std::fs::remove_file(source_path);
        Ok(())
    }

    #[test]
    fn write_features_rejects_zero_matrix_dims() -> Result<()> {
        let features = WorldFeatures {
            base_f0: 440.0,
            f0: vec![440.0],
            mgc: MatrixF64 {
                rows: 1,
                cols: 0,
                data: Vec::new(),
            },
            bap: MatrixF64 {
                rows: 1,
                cols: 1,
                data: vec![0.0],
            },
        };

        let mut temp_path = env::temp_dir();
        temp_path.push("organum_test_zero_dim_features.ogc");
        let mut source_path = env::temp_dir();
        source_path.push("organum_test_source_zero.wav");
        std::fs::write(&source_path, "fake wav data")?;

        let config = crate::config::OrganumConfig::default();

        let err = match write_features(&temp_path, &source_path, &features, 3, &config) {
            Ok(_) => anyhow::bail!("zero-dim cache shape should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("mgc_dims"));
        assert!(!temp_path.exists());

        let _ = std::fs::remove_file(source_path);
        Ok(())
    }

    #[test]
    fn read_features_rejects_oversized_cache_header_before_decompression() -> Result<()> {
        let config = crate::config::OrganumConfig::default();
        let mut temp_path = env::temp_dir();
        temp_path.push("organum_test_oversized_features.ogc");
        let mut source_path = env::temp_dir();
        source_path.push("organum_test_source_oversized.wav");
        std::fs::write(&source_path, "fake wav data")?;

        let actual_hash = compute_file_hash(&source_path);
        let mut header = test_header(&config, actual_hash);
        header.frame_count = (MAX_CACHE_FRAME_COUNT + 1) as u32;
        std::fs::write(&temp_path, header.to_bytes())?;

        let err = match read_features(&temp_path, &source_path, &config) {
            Ok(_) => anyhow::bail!("oversized cache should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("frame_count"));

        let _ = std::fs::remove_file(temp_path);
        let _ = std::fs::remove_file(source_path);
        Ok(())
    }

    #[test]
    fn read_features_rejects_decompressed_payload_larger_than_header() -> Result<()> {
        let config = crate::config::OrganumConfig::default();
        let mut source_path = env::temp_dir();
        source_path.push("organum_test_source_payload.wav");
        std::fs::write(&source_path, "fake wav data")?;

        let source_hash = compute_file_hash(&source_path);
        let header = test_header(&config, source_hash);

        let mut temp_path = env::temp_dir();
        temp_path.push("organum_test_overexpanded_features.ogc");

        let mut bytes = Vec::from(header.to_bytes());
        bytes.extend(zstd::stream::encode_all(&[42_u8][..], 1)?);
        std::fs::write(&temp_path, bytes)?;

        let err = match read_features(&temp_path, &source_path, &config) {
            Ok(_) => anyhow::bail!("overexpanded cache should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("payload size mismatch"));

        let _ = std::fs::remove_file(temp_path);
        let _ = std::fs::remove_file(source_path);
        Ok(())
    }
}
