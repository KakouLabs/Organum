use crate::config::OrganumConfig;
use std::sync::atomic::{AtomicBool, Ordering};

use super::synthesis::WarpBackend;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Cpu,
    Gpu,
}

impl Device {
    #[inline]
    pub fn as_warp_backend(self) -> WarpBackend {
        match self {
            Device::Cpu => WarpBackend::Cpu,
            Device::Gpu => WarpBackend::Gpu,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DevicePolicy {
    pub gpu_warp_enabled: bool,
    pub gpu_warp_min_frames: usize,
    pub gpu_ap_min_frames: usize,
}

static GPU_RUNTIME_DISABLED: AtomicBool = AtomicBool::new(false);
static GPU_FEATURE_WARNED: AtomicBool = AtomicBool::new(false);

pub fn mark_gpu_unavailable(reason: &str) {
    if !GPU_RUNTIME_DISABLED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "Disabling GPU warp route for this process, falling back to CPU: {}",
            reason
        );
    }
}

impl DevicePolicy {
    pub fn from_config(config: &OrganumConfig) -> Self {
        Self {
            gpu_warp_enabled: config.gpu_warp_enabled,
            gpu_warp_min_frames: config.gpu_warp_min_frames,
            gpu_ap_min_frames: config.gpu_ap_min_frames,
        }
    }

    #[inline]
    pub fn select(self, render_length: usize) -> Device {
        self.select_warp(render_length)
    }

    #[inline]
    pub fn select_warp(self, render_length: usize) -> Device {
        self.select_with_threshold(render_length, self.gpu_warp_min_frames)
    }

    #[inline]
    pub fn select_aperiodicity(self, render_length: usize) -> Device {
        self.select_with_threshold(render_length, self.gpu_ap_min_frames)
    }

    #[inline]
    fn select_with_threshold(self, render_length: usize, min_frames: usize) -> Device {
        if !self.gpu_warp_enabled {
            Device::Cpu
        } else if !cfg!(feature = "gpu-warp") {
            if !GPU_FEATURE_WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "gpu_warp_enabled=true but this binary was built without 'gpu-warp'; using CPU"
                );
            }
            Device::Cpu
        } else if GPU_RUNTIME_DISABLED.load(Ordering::Relaxed) {
            Device::Cpu
        } else if render_length >= min_frames {
            Device::Gpu
        } else {
            Device::Cpu
        }
    }
}
