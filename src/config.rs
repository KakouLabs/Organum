use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum QualityPreset {
    Classic,
    #[default]
    Balanced,
    Clear,
    BreathySafe,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrganumConfig {
    #[serde(default = "default_feature_ext")]
    pub feature_extension: String,

    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    #[serde(default = "default_frame_period")]
    pub frame_period: f32,

    #[serde(default = "default_zstd_level")]
    pub zstd_compression_level: i32,

    #[serde(default = "default_compressor_threshold")]
    pub compressor_threshold: f32,

    #[serde(default = "default_compressor_limit")]
    pub compressor_limit: f32,

    #[serde(default = "default_gpu_warp_enabled")]
    pub gpu_warp_enabled: bool,

    #[serde(default = "default_gpu_warp_min_frames")]
    pub gpu_warp_min_frames: usize,

    #[serde(default = "default_gpu_ap_min_frames")]
    pub gpu_ap_min_frames: usize,

    #[serde(default = "default_output_dither")]
    pub output_dither: bool,

    #[serde(default = "default_memory_cache_enabled")]
    pub memory_cache_enabled: bool,

    #[serde(default = "default_memory_cache_max_mb")]
    pub memory_cache_max_mb: usize,

    #[serde(default = "default_ignore_cache_hash_verification")]
    pub ignore_cache_hash_verification: bool,

    #[serde(default)]
    pub quality_preset: QualityPreset,
}

fn default_feature_ext() -> String {
    "ogc".to_string()
}
fn default_sample_rate() -> u32 {
    44100
}
fn default_frame_period() -> f32 {
    5.0
}
fn default_zstd_level() -> i32 {
    3
}
fn default_compressor_threshold() -> f32 {
    0.85
}
fn default_compressor_limit() -> f32 {
    0.99
}
fn default_gpu_warp_enabled() -> bool {
    false
}
fn default_gpu_warp_min_frames() -> usize {
    usize::MAX
}
fn default_gpu_ap_min_frames() -> usize {
    usize::MAX
}
fn default_output_dither() -> bool {
    true
}
fn default_memory_cache_enabled() -> bool {
    true
}
fn default_memory_cache_max_mb() -> usize {
    512
}
fn default_ignore_cache_hash_verification() -> bool {
    true
}

impl Default for OrganumConfig {
    fn default() -> Self {
        Self {
            feature_extension: default_feature_ext(),
            sample_rate: default_sample_rate(),
            frame_period: default_frame_period(),
            zstd_compression_level: default_zstd_level(),
            compressor_threshold: default_compressor_threshold(),
            compressor_limit: default_compressor_limit(),
            gpu_warp_enabled: default_gpu_warp_enabled(),
            gpu_warp_min_frames: default_gpu_warp_min_frames(),
            gpu_ap_min_frames: default_gpu_ap_min_frames(),
            output_dither: default_output_dither(),
            memory_cache_enabled: default_memory_cache_enabled(),
            memory_cache_max_mb: default_memory_cache_max_mb(),
            ignore_cache_hash_verification: default_ignore_cache_hash_verification(),
            quality_preset: QualityPreset::default(),
        }
    }
}

pub fn load_config() -> OrganumConfig {
    let config_path = get_config_path();
    load_config_from_path(&config_path)
}

fn load_config_from_path(config_path: &Path) -> OrganumConfig {
    if !config_path.exists() {
        return OrganumConfig::default();
    }

    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(config) = serde_yaml::from_str(&content) {
            return config;
        }
    }
    OrganumConfig::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_config_uses_defaults_without_creating_file() {
        let mut path = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        path.push(format!("organum-missing-config-{unique}.yaml"));

        let config = load_config_from_path(&path);

        assert_eq!(config.sample_rate, OrganumConfig::default().sample_rate);
        assert!(!path.exists());
    }

    #[test]
    fn existing_config_file_is_still_loaded() {
        let mut path = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        path.push(format!("organum-existing-config-{unique}.yaml"));
        fs::write(&path, "sample_rate: 48000\nframe_period: 2.5\n")
            .expect("test config should be writable");

        let config = load_config_from_path(&path);

        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.frame_period, 2.5);
        fs::remove_file(path).expect("test config should be removable");
    }
}

static GLOBAL_CONFIG: OnceLock<OrganumConfig> = OnceLock::new();

/// 전역 설정. 첫 호출 시 로드 후 캐싱.
pub fn global_config() -> &'static OrganumConfig {
    GLOBAL_CONFIG.get_or_init(load_config)
}

fn get_config_path() -> PathBuf {
    if let Ok(mut exe_dir) = std::env::current_exe() {
        exe_dir.pop();
        return exe_dir.join("organum.yaml");
    }
    PathBuf::from("organum.yaml")
}
