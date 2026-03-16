use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::resampler::{
    common::utils::to_feature_path,
    io::audio::read_audio,
    io::features::{generate_features, read_features, write_features},
    types::WorldFeaturesOwned,
};

const DEFAULT_MEMORY_CACHE_BYTES: usize = 256 * 1024 * 1024;

fn configured_capacity_bytes(config: &crate::config::OrganumConfig) -> usize {
    config
        .memory_cache_max_mb
        .saturating_mul(1024)
        .saturating_mul(1024)
        .max(1)
}

struct FeatureMemoryCache {
    capacity_bytes: usize,
    used_bytes: usize,
    map: HashMap<PathBuf, Arc<WorldFeaturesOwned>>,
    order: VecDeque<PathBuf>,
}

impl FeatureMemoryCache {
    fn new(capacity_bytes: usize) -> Self {
        Self {
            capacity_bytes,
            used_bytes: 0,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &Path) -> Option<Arc<WorldFeaturesOwned>> {
        let k = key.to_path_buf();
        let v = self.map.get(&k)?.clone();
        self.touch(&k);
        Some(v)
    }

    fn insert(&mut self, key: PathBuf, value: Arc<WorldFeaturesOwned>) {
        if let Some(prev) = self.map.remove(&key) {
            self.used_bytes = self.used_bytes.saturating_sub(prev.byte_size());
            self.remove_from_order(&key);
        }

        let size = value.byte_size();
        self.map.insert(key.clone(), value);
        self.order.push_back(key);
        self.used_bytes = self.used_bytes.saturating_add(size);

        self.evict_if_needed();
    }

    fn touch(&mut self, key: &PathBuf) {
        self.remove_from_order(key);
        self.order.push_back(key.clone());
    }

    fn remove_from_order(&mut self, key: &PathBuf) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
    }

    fn evict_if_needed(&mut self) {
        while self.used_bytes > self.capacity_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(v) = self.map.remove(&oldest) {
                self.used_bytes = self.used_bytes.saturating_sub(v.byte_size());
            }
        }
    }
}

static FEATURE_MEMORY_CACHE: OnceLock<Mutex<FeatureMemoryCache>> = OnceLock::new();

fn memory_cache() -> &'static Mutex<FeatureMemoryCache> {
    FEATURE_MEMORY_CACHE
        .get_or_init(|| Mutex::new(FeatureMemoryCache::new(DEFAULT_MEMORY_CACHE_BYTES)))
}

fn reconfigure_memory_cache(config: &crate::config::OrganumConfig) {
    if let Ok(mut cache) = memory_cache().lock() {
        let desired_capacity = configured_capacity_bytes(config);
        if cache.capacity_bytes != desired_capacity {
            *cache = FeatureMemoryCache::new(desired_capacity);
        }
    }
}

pub fn get_cached_features(path: &Path) -> Option<Arc<WorldFeaturesOwned>> {
    let Ok(mut cache) = memory_cache().lock() else {
        return None;
    };
    cache.get(path)
}

pub fn put_cached_features(path: PathBuf, features: Arc<WorldFeaturesOwned>) {
    if let Ok(mut cache) = memory_cache().lock() {
        cache.insert(path, features);
    }
}

pub fn clear_feature_memory_cache() {
    if let Ok(mut cache) = memory_cache().lock() {
        *cache = FeatureMemoryCache::new(DEFAULT_MEMORY_CACHE_BYTES);
    }
}

pub fn load_features_cached(
    input_path: &Path,
    feature_path: &Path,
    config: &crate::config::OrganumConfig,
) -> Result<Arc<WorldFeaturesOwned>> {
    if config.memory_cache_enabled {
        reconfigure_memory_cache(config);
        if let Some(hit) = get_cached_features(feature_path) {
            tracing::debug!("memory cache hit: {:?}", feature_path);
            return Ok(hit);
        }
    }

    if feature_path.exists() {
        match read_features(feature_path, config) {
            Ok(features) => {
                tracing::debug!("disk cache hit: {:?}", feature_path);
                let owned = Arc::new(WorldFeaturesOwned::from_world_features(&features));
                if config.memory_cache_enabled {
                    put_cached_features(feature_path.to_path_buf(), Arc::clone(&owned));
                }
                return Ok(owned);
            }
            Err(e) => {
                tracing::warn!(
                    "cache miss: cache {:?} is invalid ({}), regenerating",
                    feature_path,
                    e
                );
            }
        }
    } else {
        tracing::debug!("cache miss: no cache file for {:?}", input_path);
    }

    let audio = read_audio(input_path, config.sample_rate)?;
    let features = generate_features(&audio, config.sample_rate, config.frame_period)?;
    let _ = write_features(
        feature_path,
        &features,
        config.zstd_compression_level,
        config,
    );

    let owned = Arc::new(WorldFeaturesOwned::from_world_features(&features));
    if config.memory_cache_enabled {
        put_cached_features(feature_path.to_path_buf(), Arc::clone(&owned));
    }
    Ok(owned)
}

pub fn generate_and_cache_features(
    input_path: &Path,
    config: &crate::config::OrganumConfig,
) -> Result<()> {
    let start_time = Instant::now();
    let feature_path = to_feature_path(input_path, &config.feature_extension);

    let _ = load_features_cached(input_path, &feature_path, config)?;

    tracing::info!(
        "Feature extraction & cache generation complete for {:?} in {:?}",
        input_path,
        start_time.elapsed()
    );
    Ok(())
}
