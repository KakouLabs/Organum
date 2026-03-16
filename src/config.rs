use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
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
    pub frame_period: f64,

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

    #[serde(default)]
    pub quality_preset: QualityPreset,
}

fn default_feature_ext() -> String {
    "ogc".to_string()
}
fn default_sample_rate() -> u32 {
    44100
}
fn default_frame_period() -> f64 {
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
    256
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
            quality_preset: QualityPreset::default(),
        }
    }
}

pub fn load_config() -> OrganumConfig {
    let config_path = get_config_path();

    if !config_path.exists() {
        let default_config = OrganumConfig::default();
        let yaml_content = format!(
            "# Organum Configuration File\n\
             # You can edit these values to customize engine behavior.\n\n\
             # Extension for cached feature files (default: ogc)\n\
             feature_extension: \"{}\"\n\n\
             # Processing sample rate in Hz (default: 44100)\n\
             sample_rate: {}\n\n\
             # WORLD frame period in ms (default: 5.0)\n\
             frame_period: {:.1}\n\n\
             # Zstd compression level for cache files (1-22, default: 3)\n\
             zstd_compression_level: {}\n\n\
             # Wavtool compressor threshold (default: 0.85)\n\
             compressor_threshold: {:.2}\n\n\
             # Wavtool compressor limit (default: 0.99)\n\
             compressor_limit: {:.2}\n\n\
             # Enable experimental GPU route for warp_spectrum (default: false)\n\
             gpu_warp_enabled: {}\n\n\
              # Minimum render frames before trying GPU warp route (default: disabled/CPU-only)\n\
              gpu_warp_min_frames: {}\n\n\
              # Minimum render frames before trying GPU aperiodicity route (default: disabled/CPU-only)\n\
             gpu_ap_min_frames: {}\n\
             # Enable output dithering/noise-shaping on WAV write (default: true)\n\
             output_dither: {}\n\
             # Enable in-process feature memory cache (default: true)\n\
              memory_cache_enabled: {}\n\
              # Maximum feature memory cache size in MiB (default: 256)\n\
             memory_cache_max_mb: {}\n\
             # Quality preset: classic, balanced, clear, breathy-safe (default: balanced)\n\
             quality_preset: \"{}\"\n",
            default_config.feature_extension,
            default_config.sample_rate,
            default_config.frame_period,
            default_config.zstd_compression_level,
            default_config.compressor_threshold,
            default_config.compressor_limit,
            default_config.gpu_warp_enabled,
            default_config.gpu_warp_min_frames,
            default_config.gpu_ap_min_frames,
            default_config.output_dither,
            default_config.memory_cache_enabled,
            default_config.memory_cache_max_mb,
            match default_config.quality_preset {
                QualityPreset::Classic => "classic",
                QualityPreset::Balanced => "balanced",
                QualityPreset::Clear => "clear",
                QualityPreset::BreathySafe => "breathy-safe",
            },
        );
        let _ = fs::write(&config_path, yaml_content);
        return default_config;
    }

    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(config) = serde_yaml::from_str(&content) {
            return config;
        }
    }
    OrganumConfig::default()
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
