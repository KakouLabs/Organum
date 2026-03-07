use anyhow::Result;
use std::path::Path;
use std::time::Instant;

use crate::resampler::{
    common::utils::to_feature_path,
    io::audio::read_audio,
    io::features::{generate_features, read_features, write_features},
};

pub fn generate_and_cache_features(
    input_path: &Path,
    config: &crate::config::OrganumConfig,
) -> Result<()> {
    let start_time = Instant::now();
    let feature_path = to_feature_path(input_path, &config.feature_extension);

    if feature_path.exists() {
        if read_features(&feature_path).is_ok() {
            tracing::debug!("cache hit: read-only verification for {:?}", input_path);
            return Ok(());
        }
        tracing::warn!(
            "cache miss: cache {:?} is corrupted/outdated, regenerating",
            input_path
        );
    } else {
        tracing::debug!("cache miss: no cache file for {:?}", input_path);
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
