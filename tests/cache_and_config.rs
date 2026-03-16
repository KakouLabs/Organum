use organum::config::OrganumConfig;
use organum::resampler::types::{
    CacheV5Header, MatrixF64, CACHE_V5_FORMAT_VERSION, CACHE_V5_MAGIC,
};

#[test]
fn cache_header_roundtrip_is_stable() {
    let header = CacheV5Header {
        magic: CACHE_V5_MAGIC,
        format_version: CACHE_V5_FORMAT_VERSION,
        cache_key: 123456789,
        sample_rate: 44100,
        frame_period_micros: 5000,
        frame_count: 321,
        mgc_dims: 60,
        bap_dims: 5,
        payload_size: 9999,
        _reserved: 0,
    };

    let bytes = header.to_bytes();
    let decoded = CacheV5Header::from_bytes(&bytes);

    let decoded_format_version = decoded.format_version;
    let header_format_version = header.format_version;
    let decoded_cache_key = decoded.cache_key;
    let header_cache_key = header.cache_key;
    let decoded_sample_rate = decoded.sample_rate;
    let header_sample_rate = header.sample_rate;
    let decoded_frame_period_micros = decoded.frame_period_micros;
    let header_frame_period_micros = header.frame_period_micros;
    let decoded_frame_count = decoded.frame_count;
    let header_frame_count = header.frame_count;
    let decoded_mgc_dims = decoded.mgc_dims;
    let header_mgc_dims = header.mgc_dims;
    let decoded_bap_dims = decoded.bap_dims;
    let header_bap_dims = header.bap_dims;
    let decoded_payload_size = decoded.payload_size;
    let header_payload_size = header.payload_size;

    assert_eq!(decoded.magic, header.magic);
    assert_eq!(decoded_format_version, header_format_version);
    assert_eq!(decoded_cache_key, header_cache_key);
    assert_eq!(decoded_sample_rate, header_sample_rate);
    assert_eq!(decoded_frame_period_micros, header_frame_period_micros);
    assert_eq!(decoded_frame_count, header_frame_count);
    assert_eq!(decoded_mgc_dims, header_mgc_dims);
    assert_eq!(decoded_bap_dims, header_bap_dims);
    assert_eq!(decoded_payload_size, header_payload_size);
}

#[test]
fn matrix_from_vecs_rejects_ragged_input() {
    let bad = vec![vec![1.0, 2.0], vec![3.0]];
    assert!(MatrixF64::from_vecs(&bad).is_err());
}

#[test]
fn config_defaults_include_memory_cache_settings() {
    let config = OrganumConfig::default();
    assert!(config.memory_cache_enabled);
    assert_eq!(config.memory_cache_max_mb, 256);
    assert!(config.output_dither);
    assert_eq!(format!("{:?}", config.quality_preset), "Balanced");
}
