use rayon::prelude::*;
use std::cell::RefCell;
#[cfg(feature = "gpu-warp")]
use std::collections::VecDeque;
#[cfg(not(feature = "gpu-warp"))]
use std::sync::atomic::Ordering;
#[cfg(feature = "gpu-warp")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "gpu-warp")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

pub fn synthesize(
    f0: &Vec<f64>,
    sp: &mut Vec<Vec<f64>>,
    ap: &mut Vec<Vec<f64>>,
    sample_rate: u32,
    frame_period: f64,
) -> Vec<f64> {
    rsworld::synthesis(f0, sp, ap, frame_period, sample_rate as i32)
}

#[derive(Clone, Copy, Debug)]
pub struct QualityProfile {
    pub high_pitch_warp_relax: f64,
    pub high_pitch_tilt_cap: f64,
    pub tilt_strength: f64,
    pub presence_max_gain: f64,
    pub unvoiced_spike_threshold: f64,
    pub unvoiced_spike_blend: f64,
    pub breath_floor_base: f64,
    pub breath_floor_onset_boost: f64,
    pub voiced_high_cap_base: f64,
    pub voiced_high_cap_onset_drop: f64,
}

impl QualityProfile {
    pub fn from_preset(preset: crate::config::QualityPreset) -> Self {
        match preset {
            crate::config::QualityPreset::Classic => Self {
                high_pitch_warp_relax: 0.0,
                high_pitch_tilt_cap: 1.0,
                tilt_strength: 1.2,
                presence_max_gain: 0.0,
                unvoiced_spike_threshold: 1.0,
                unvoiced_spike_blend: 0.0,
                breath_floor_base: 0.0,
                breath_floor_onset_boost: 0.0,
                voiced_high_cap_base: 1.0,
                voiced_high_cap_onset_drop: 0.0,
            },
            crate::config::QualityPreset::Balanced => Self {
                high_pitch_warp_relax: 0.35,
                high_pitch_tilt_cap: 0.9,
                tilt_strength: 0.9,
                presence_max_gain: 0.08,
                unvoiced_spike_threshold: 0.16,
                unvoiced_spike_blend: 0.65,
                breath_floor_base: 0.04,
                breath_floor_onset_boost: 0.14,
                voiced_high_cap_base: 0.985,
                voiced_high_cap_onset_drop: 0.08,
            },
            crate::config::QualityPreset::Clear => Self {
                high_pitch_warp_relax: 0.45,
                high_pitch_tilt_cap: 0.82,
                tilt_strength: 0.8,
                presence_max_gain: 0.12,
                unvoiced_spike_threshold: 0.14,
                unvoiced_spike_blend: 0.72,
                breath_floor_base: 0.05,
                breath_floor_onset_boost: 0.12,
                voiced_high_cap_base: 0.97,
                voiced_high_cap_onset_drop: 0.10,
            },
            crate::config::QualityPreset::BreathySafe => Self {
                high_pitch_warp_relax: 0.3,
                high_pitch_tilt_cap: 0.88,
                tilt_strength: 0.9,
                presence_max_gain: 0.05,
                unvoiced_spike_threshold: 0.12,
                unvoiced_spike_blend: 0.8,
                breath_floor_base: 0.07,
                breath_floor_onset_boost: 0.16,
                voiced_high_cap_base: 0.975,
                voiced_high_cap_onset_drop: 0.08,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SimdMode {
    Auto,
    On,
    Off,
}

fn parse_simd_mode() -> SimdMode {
    match std::env::var("ORGANUM_AP_SIMD") {
        Ok(v) => {
            let s = v.trim().to_ascii_lowercase();
            match s.as_str() {
                "on" | "1" | "true" => SimdMode::On,
                "off" | "0" | "false" => SimdMode::Off,
                _ => SimdMode::Auto,
            }
        }
        Err(_) => SimdMode::Auto,
    }
}

fn cpu_supports_ap_simd() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

fn ap_simd_enabled() -> bool {
    static AP_SIMD_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static AP_SIMD_LOGGED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    let enabled = *AP_SIMD_ENABLED.get_or_init(|| {
        let mode = parse_simd_mode();
        let supported = cpu_supports_ap_simd();
        match mode {
            SimdMode::Off => false,
            SimdMode::On => {
                if !supported {
                    tracing::warn!(
                        "ORGANUM_AP_SIMD=on but AVX2 is unavailable; using scalar CPU path"
                    );
                }
                supported
            }
            SimdMode::Auto => supported,
        }
    });

    if !AP_SIMD_LOGGED.swap(true, Ordering::Relaxed) {
        tracing::info!(
            "aperiodicity CPU SIMD route: {} (ORGANUM_AP_SIMD={})",
            if enabled { "enabled" } else { "disabled" },
            std::env::var("ORGANUM_AP_SIMD").unwrap_or_else(|_| "auto".to_string())
        );
    }

    enabled
}

#[inline]
fn apply_aperiodicity_frame_scalar(
    frame: &mut [f64],
    is_voiced: bool,
    params: CpuAperiodicityParams,
) {
    let h_factor = params.h_factor;
    let c_factor = params.c_factor;
    let breathiness_factor = params.breathiness_factor;
    let b_scale = params.b_scale;
    let onset_breath_factor = params.onset_breath_factor;

    for a in frame.iter_mut() {
        if is_voiced {
            if h_factor > 0.0 {
                *a *= 1.0 - h_factor;
            }
        } else if c_factor > 0.0 {
            *a *= 1.0 - c_factor;
        }

        if breathiness_factor > 0.0 {
            *a += (1.0 - *a) * breathiness_factor;
        } else if breathiness_factor < 0.0 {
            *a *= b_scale;
        }

        if onset_breath_factor > 0.0 {
            *a += (1.0 - *a) * onset_breath_factor;
        }

        *a = a.clamp(0.0, 1.0);
    }

    stabilize_aperiodicity_frame(
        frame,
        is_voiced,
        onset_breath_factor,
        QualityProfile::from_preset(crate::config::QualityPreset::Balanced),
    );
}

#[inline]
fn apply_aperiodicity_frame_scalar_f32(
    frame: &mut [f32],
    is_voiced: bool,
    params: CpuAperiodicityParamsF32,
) {
    let h_factor = params.h_factor;
    let c_factor = params.c_factor;
    let breathiness_factor = params.breathiness_factor;
    let b_scale = params.b_scale;
    let onset_breath_factor = params.onset_breath_factor;

    for a in frame.iter_mut() {
        if is_voiced {
            if h_factor > 0.0 {
                *a *= 1.0 - h_factor;
            }
        } else if c_factor > 0.0 {
            *a *= 1.0 - c_factor;
        }

        if breathiness_factor > 0.0 {
            *a += (1.0 - *a) * breathiness_factor;
        } else if breathiness_factor < 0.0 {
            *a *= b_scale;
        }

        if onset_breath_factor > 0.0 {
            *a += (1.0 - *a) * onset_breath_factor;
        }

        *a = a.clamp(0.0, 1.0);
    }

    stabilize_aperiodicity_frame_f32(
        frame,
        is_voiced,
        onset_breath_factor,
        QualityProfile::from_preset(crate::config::QualityPreset::Balanced),
    );
}

#[inline]
fn smooth_spiky_bins(frame: &mut [f64], start: usize, spike_threshold: f64, blend: f64) {
    if frame.len() < 3 || start + 1 >= frame.len() {
        return;
    }

    for i in (start + 1)..(frame.len() - 1) {
        let neighbor_avg = 0.5 * (frame[i - 1] + frame[i + 1]);
        let spike = frame[i] - neighbor_avg;
        if frame[i] > 0.85 && spike > spike_threshold {
            frame[i] = (frame[i] * (1.0 - blend) + neighbor_avg * blend).clamp(0.0, 1.0);
        }
    }
}

#[inline]
fn smooth_spiky_bins_f32(frame: &mut [f32], start: usize, spike_threshold: f32, blend: f32) {
    if frame.len() < 3 || start + 1 >= frame.len() {
        return;
    }

    for i in (start + 1)..(frame.len() - 1) {
        let neighbor_avg = 0.5 * (frame[i - 1] + frame[i + 1]);
        let spike = frame[i] - neighbor_avg;
        if frame[i] > 0.85 && spike > spike_threshold {
            frame[i] = (frame[i] * (1.0 - blend) + neighbor_avg * blend).clamp(0.0, 1.0);
        }
    }
}

#[inline]
fn stabilize_aperiodicity_frame(
    frame: &mut [f64],
    is_voiced: bool,
    onset_breath_factor: f64,
    profile: QualityProfile,
) {
    if frame.is_empty() {
        return;
    }

    for v in frame.iter_mut() {
        if !v.is_finite() {
            *v = 0.0;
        }
        *v = v.clamp(0.0, 1.0);
    }

    let len = frame.len();
    if len < 8 {
        return;
    }

    let hf_start = len * 3 / 4;
    if hf_start >= len {
        return;
    }

    if !is_voiced {
        let hf = &frame[hf_start..];
        let hf_mean = hf.iter().copied().sum::<f64>() / hf.len() as f64;
        let hf_max = hf.iter().copied().fold(0.0_f64, f64::max);
        let spike_amount = hf_max - hf_mean;

        if spike_amount > profile.unvoiced_spike_threshold {
            smooth_spiky_bins(
                frame,
                hf_start,
                profile.unvoiced_spike_threshold,
                profile.unvoiced_spike_blend,
            );
        }

        let breath_floor = (profile.breath_floor_base
            + onset_breath_factor * profile.breath_floor_onset_boost)
            .clamp(0.0, 0.18);
        let low_band_end = (len / 6).max(1);
        for v in &mut frame[..low_band_end] {
            *v = v.max(breath_floor);
        }
    } else {
        let hf_cap = (profile.voiced_high_cap_base
            - onset_breath_factor * profile.voiced_high_cap_onset_drop)
            .clamp(0.88, 1.0);
        for v in &mut frame[hf_start..] {
            *v = v.min(hf_cap);
        }
    }
}

#[inline]
fn stabilize_aperiodicity_frame_f32(
    frame: &mut [f32],
    is_voiced: bool,
    onset_breath_factor: f32,
    profile: QualityProfile,
) {
    if frame.is_empty() {
        return;
    }

    for v in frame.iter_mut() {
        if !v.is_finite() {
            *v = 0.0;
        }
        *v = v.clamp(0.0, 1.0);
    }

    let len = frame.len();
    if len < 8 {
        return;
    }

    let hf_start = len * 3 / 4;
    if hf_start >= len {
        return;
    }

    if !is_voiced {
        let hf = &frame[hf_start..];
        let hf_mean = hf.iter().copied().sum::<f32>() / hf.len() as f32;
        let hf_max = hf.iter().copied().fold(0.0_f32, f32::max);
        let spike_amount = hf_max - hf_mean;

        if spike_amount > profile.unvoiced_spike_threshold as f32 {
            smooth_spiky_bins_f32(
                frame,
                hf_start,
                profile.unvoiced_spike_threshold as f32,
                profile.unvoiced_spike_blend as f32,
            );
        }

        let breath_floor = ((profile.breath_floor_base as f32)
            + onset_breath_factor * profile.breath_floor_onset_boost as f32)
            .clamp(0.0, 0.18);
        let low_band_end = (len / 6).max(1);
        for v in &mut frame[..low_band_end] {
            *v = v.max(breath_floor);
        }
    } else {
        let hf_cap = ((profile.voiced_high_cap_base as f32)
            - onset_breath_factor * profile.voiced_high_cap_onset_drop as f32)
            .clamp(0.88, 1.0);
        for v in &mut frame[hf_start..] {
            *v = v.min(hf_cap);
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn apply_aperiodicity_frame_avx2_f32(
    frame: &mut [f32],
    is_voiced: bool,
    params: CpuAperiodicityParamsF32,
) {
    let h_factor = params.h_factor;
    let c_factor = params.c_factor;
    let breathiness_factor = params.breathiness_factor;
    let b_scale = params.b_scale;
    let onset_breath_factor = params.onset_breath_factor;

    let mut i = 0usize;
    let len = frame.len();

    let zero = _mm256_set1_ps(0.0);
    let one = _mm256_set1_ps(1.0);

    let route_mul = if is_voiced {
        if h_factor > 0.0 {
            _mm256_set1_ps(1.0 - h_factor)
        } else {
            one
        }
    } else if c_factor > 0.0 {
        _mm256_set1_ps(1.0 - c_factor)
    } else {
        one
    };

    let breath_pos = breathiness_factor > 0.0;
    let breath_neg = breathiness_factor < 0.0;
    let breath_mul = _mm256_set1_ps(1.0 - breathiness_factor);
    let breath_add = _mm256_set1_ps(breathiness_factor);
    let b_scale_v = _mm256_set1_ps(b_scale);

    let onset_enabled = onset_breath_factor > 0.0;
    let onset_mul = _mm256_set1_ps(1.0 - onset_breath_factor);
    let onset_add = _mm256_set1_ps(onset_breath_factor);

    while i + 8 <= len {
        let ptr = frame.as_mut_ptr().add(i);
        let mut x = _mm256_loadu_ps(ptr);

        x = _mm256_mul_ps(x, route_mul);

        if breath_pos {
            x = _mm256_add_ps(_mm256_mul_ps(x, breath_mul), breath_add);
        } else if breath_neg {
            x = _mm256_mul_ps(x, b_scale_v);
        }

        if onset_enabled {
            x = _mm256_add_ps(_mm256_mul_ps(x, onset_mul), onset_add);
        }

        x = _mm256_max_ps(x, zero);
        x = _mm256_min_ps(x, one);
        _mm256_storeu_ps(ptr, x);

        i += 8;
    }

    if i < len {
        apply_aperiodicity_frame_scalar_f32(&mut frame[i..], is_voiced, params);
    }
}

#[inline]
fn apply_aperiodicity_frame_f32(
    frame: &mut [f32],
    is_voiced: bool,
    params: CpuAperiodicityParamsF32,
    use_simd: bool,
) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if use_simd {
            unsafe {
                apply_aperiodicity_frame_avx2_f32(frame, is_voiced, params);
            }
            return;
        }
    }

    apply_aperiodicity_frame_scalar_f32(frame, is_voiced, params);
}

#[inline]
fn apply_aperiodicity_frame_simd_f32_with_scratch(
    frame: &mut [f64],
    is_voiced: bool,
    params: CpuAperiodicityParams,
    scratch: &mut Vec<f32>,
) {
    if scratch.len() < frame.len() {
        scratch.resize(frame.len(), 0.0);
    }
    let buf = &mut scratch[..frame.len()];
    for (dst, src) in buf.iter_mut().zip(frame.iter()) {
        *dst = *src as f32;
    }

    apply_aperiodicity_frame_f32(buf, is_voiced, params.to_f32(), true);

    for (dst, src) in frame.iter_mut().zip(buf.iter()) {
        *dst = *src as f64;
    }
}

#[derive(Clone, Copy)]
struct CpuAperiodicityParams {
    h_factor: f64,
    c_factor: f64,
    breathiness_factor: f64,
    b_scale: f64,
    onset_breath_factor: f64,
}

impl CpuAperiodicityParams {
    fn to_f32(self) -> CpuAperiodicityParamsF32 {
        CpuAperiodicityParamsF32 {
            h_factor: self.h_factor as f32,
            c_factor: self.c_factor as f32,
            breathiness_factor: self.breathiness_factor as f32,
            b_scale: self.b_scale as f32,
            onset_breath_factor: self.onset_breath_factor as f32,
        }
    }
}

#[derive(Clone, Copy)]
struct CpuAperiodicityParamsF32 {
    h_factor: f32,
    c_factor: f32,
    breathiness_factor: f32,
    b_scale: f32,
    onset_breath_factor: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarpBackend {
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GpuWarpStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_reallocs: u64,
    pub buffer_allocations: u64,
    pub lut_uploads: u64,
    pub map_errors: u64,
    pub cache_return_lock_failures: u64,
    pub chunk_dispatches: u64,
}

#[cfg(feature = "gpu-warp")]
static GPU_WARP_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "gpu-warp")]
static GPU_WARP_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "gpu-warp")]
static GPU_WARP_CACHE_REALLOCS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "gpu-warp")]
static GPU_WARP_BUFFER_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "gpu-warp")]
static GPU_WARP_LUT_UPLOADS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "gpu-warp")]
static GPU_WARP_MAP_ERRORS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "gpu-warp")]
static GPU_WARP_CACHE_RETURN_LOCK_FAILURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "gpu-warp")]
static GPU_WARP_CHUNK_DISPATCHES: AtomicU64 = AtomicU64::new(0);

pub fn reset_gpu_warp_stats() {
    #[cfg(feature = "gpu-warp")]
    {
        GPU_WARP_CACHE_HITS.store(0, Ordering::Relaxed);
        GPU_WARP_CACHE_MISSES.store(0, Ordering::Relaxed);
        GPU_WARP_CACHE_REALLOCS.store(0, Ordering::Relaxed);
        GPU_WARP_BUFFER_ALLOCATIONS.store(0, Ordering::Relaxed);
        GPU_WARP_LUT_UPLOADS.store(0, Ordering::Relaxed);
        GPU_WARP_MAP_ERRORS.store(0, Ordering::Relaxed);
        GPU_WARP_CACHE_RETURN_LOCK_FAILURES.store(0, Ordering::Relaxed);
        GPU_WARP_CHUNK_DISPATCHES.store(0, Ordering::Relaxed);
    }
}

pub fn gpu_warp_stats() -> GpuWarpStats {
    #[cfg(feature = "gpu-warp")]
    {
        GpuWarpStats {
            cache_hits: GPU_WARP_CACHE_HITS.load(Ordering::Relaxed),
            cache_misses: GPU_WARP_CACHE_MISSES.load(Ordering::Relaxed),
            cache_reallocs: GPU_WARP_CACHE_REALLOCS.load(Ordering::Relaxed),
            buffer_allocations: GPU_WARP_BUFFER_ALLOCATIONS.load(Ordering::Relaxed),
            lut_uploads: GPU_WARP_LUT_UPLOADS.load(Ordering::Relaxed),
            map_errors: GPU_WARP_MAP_ERRORS.load(Ordering::Relaxed),
            cache_return_lock_failures: GPU_WARP_CACHE_RETURN_LOCK_FAILURES.load(Ordering::Relaxed),
            chunk_dispatches: GPU_WARP_CHUNK_DISPATCHES.load(Ordering::Relaxed),
        }
    }

    #[cfg(not(feature = "gpu-warp"))]
    {
        GpuWarpStats::default()
    }
}

#[derive(Clone)]
pub struct WarpLut {
    pub idx_floor: Vec<usize>,
    pub frac: Vec<f32>,
    pub clamped: Vec<u8>,
}

thread_local! {
    static WARP_APPLY_SCRATCH: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct WarpLutKey {
    len: usize,
    sample_rate_hz: u32,
    factor_bucket: i32,
}

thread_local! {
    static WARP_LUT_CACHE: RefCell<Vec<(WarpLutKey, WarpLut)>> = const { RefCell::new(Vec::new()) };
}

impl WarpLut {
    pub fn new(len: usize, fs: f64, factor: f64) -> Self {
        let df = fs / ((len - 1) as f64 * 2.0);
        let last = (len - 1) as f64;

        let mut idx_floor = Vec::with_capacity(len);
        let mut frac = Vec::with_capacity(len);
        let mut clamped = Vec::with_capacity(len);

        for i in 0..len {
            let f_dst = i as f64 * df;
            let m_dst = 2595.0 * (1.0 + f_dst / 700.0).log10();
            let m_src = m_dst * factor;
            let f_src = 700.0 * (10.0f64.powf(m_src / 2595.0) - 1.0);
            let src_idx = f_src / df;

            if src_idx >= last {
                idx_floor.push(len - 1);
                frac.push(0.0);
                clamped.push(1);
            } else {
                let fl = src_idx as usize;
                idx_floor.push(fl);
                frac.push((src_idx - fl as f64) as f32);
                clamped.push(0);
            }
        }

        Self {
            idx_floor,
            frac,
            clamped,
        }
    }

    pub fn cached(len: usize, fs: f64, factor: f64) -> Self {
        #[inline]
        fn factor_bucket(factor: f64) -> i32 {
            (factor * 500.0).round() as i32
        }

        let key = WarpLutKey {
            len,
            sample_rate_hz: fs.round() as u32,
            factor_bucket: factor_bucket(factor),
        };

        WARP_LUT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(index) = cache.iter().position(|(cached_key, _)| *cached_key == key) {
                let (_, lut) = cache.remove(index);
                let cloned = lut.clone();
                cache.push((key, lut));
                return cloned;
            }

            let lut = Self::new(len, fs, factor);
            if cache.len() >= 16 {
                cache.remove(0);
            }
            cache.push((key, lut.clone()));
            lut
        })
    }

    #[inline]
    pub fn apply(&self, in_out: &mut Vec<f64>) {
        WARP_APPLY_SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            self.apply_with_scratch(in_out.as_mut_slice(), &mut scratch);
        });
    }

    #[inline]
    pub fn apply_with_scratch(&self, in_out: &mut [f64], scratch: &mut Vec<f64>) {
        scratch.clear();
        scratch.extend_from_slice(in_out);

        let last_val = *scratch.last().unwrap_or(&0.0);

        for (i, v) in in_out.iter_mut().enumerate() {
            if self.clamped[i] != 0 {
                *v = last_val;
            } else {
                let fl = self.idx_floor[i];
                let t = self.frac[i] as f64;
                *v = scratch[fl] * (1.0 - t) + scratch[fl + 1] * t;
            }
        }
    }
}

#[inline]
pub fn apply_warp_cpu_batch(frames: &mut [Vec<f64>], lut: &WarpLut) {
    const PAR_THRESHOLD: usize = 2048;

    if frames.len() < PAR_THRESHOLD {
        let mut scratch = Vec::new();
        for frame in frames.iter_mut() {
            lut.apply_with_scratch(frame.as_mut_slice(), &mut scratch);
        }
    } else {
        frames
            .par_iter_mut()
            .for_each_init(Vec::new, |scratch, frame| {
                lut.apply_with_scratch(frame.as_mut_slice(), scratch);
            });
    }
}

pub fn warp_spectrum(sp: &mut Vec<f64>, fs: f64, factor: f64) {
    if (factor - 1.0).abs() < 0.001 {
        return;
    }
    let lut = WarpLut::cached(sp.len(), fs, factor);
    lut.apply(sp);
}

#[inline]
pub fn apply_warp_with_backend(sp: &mut Vec<f64>, lut: &WarpLut, backend: WarpBackend) {
    match backend {
        WarpBackend::Cpu => lut.apply(sp),
        WarpBackend::Gpu => lut.apply(sp),
    }
}

#[inline]
pub fn try_apply_warp_batch_with_backend(
    frames: &mut [Vec<f64>],
    lut: &WarpLut,
    backend: WarpBackend,
) -> Result<(), String> {
    match backend {
        WarpBackend::Cpu => {
            apply_warp_cpu_batch(frames, lut);
            Ok(())
        }
        WarpBackend::Gpu => try_apply_warp_gpu_batch(frames, lut),
    }
}

pub fn try_apply_warp_gpu_batch(frames: &mut [Vec<f64>], lut: &WarpLut) -> Result<(), String> {
    #[cfg(feature = "gpu-warp")]
    {
        pollster::block_on(run_wgpu_warp_batch(frames, lut))
    }
    #[cfg(not(feature = "gpu-warp"))]
    {
        let _ = frames;
        let _ = lut;
        Err("gpu-warp feature is disabled at build time".to_string())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_aperiodicity_cpu_batch(
    ap_render: &mut [Vec<f64>],
    vuv_render: &[u8],
    onset_fadein_frames: usize,
    h_factor: f64,
    c_factor: f64,
    breathiness_factor: f64,
    b_scale: f64,
    quality_profile: QualityProfile,
) {
    const ONSET_BREATH_MAX: f64 = 0.6;
    const SIMD_F32_CONVERT_MIN_BINS: usize = 512;

    #[inline]
    fn onset_breath_factor_at(index: usize, onset_fadein_frames: usize) -> f64 {
        if onset_fadein_frames > 0 && index < onset_fadein_frames {
            let progress = index as f64 / onset_fadein_frames as f64;
            (1.0 - (1.0 - (progress * std::f64::consts::PI).cos()) * 0.5) * ONSET_BREATH_MAX
        } else {
            0.0
        }
    }

    #[inline]
    fn params_for_frame(
        index: usize,
        onset_fadein_frames: usize,
        h_factor: f64,
        c_factor: f64,
        breathiness_factor: f64,
        b_scale: f64,
    ) -> CpuAperiodicityParams {
        CpuAperiodicityParams {
            h_factor,
            c_factor,
            breathiness_factor,
            b_scale,
            onset_breath_factor: onset_breath_factor_at(index, onset_fadein_frames),
        }
    }

    let use_simd = ap_simd_enabled();

    if use_simd && !ap_render.is_empty() {
        let bins = ap_render[0].len();
        let homogeneous = bins >= SIMD_F32_CONVERT_MIN_BINS
            && ap_render.iter().all(|f| f.len() == bins)
            && bins > 0;

        if homogeneous {
            let frame_count = ap_render.len();
            let mut flat = Vec::with_capacity(frame_count * bins);
            for frame in ap_render.iter() {
                flat.extend(frame.iter().map(|&v| v as f32));
            }

            let onset: Vec<f32> = (0..frame_count)
                .map(|i| onset_breath_factor_at(i, onset_fadein_frames) as f32)
                .collect();

            const PAR_THRESHOLD: usize = 2048;
            if frame_count < PAR_THRESHOLD {
                for (i, chunk) in flat.chunks_mut(bins).enumerate() {
                    let params_f32 = CpuAperiodicityParamsF32 {
                        h_factor: h_factor as f32,
                        c_factor: c_factor as f32,
                        breathiness_factor: breathiness_factor as f32,
                        b_scale: b_scale as f32,
                        onset_breath_factor: onset[i],
                    };
                    let is_voiced = vuv_render.get(i).copied().unwrap_or(0) != 0;
                    apply_aperiodicity_frame_f32(chunk, is_voiced, params_f32, true);
                    stabilize_aperiodicity_frame_f32(chunk, is_voiced, onset[i], quality_profile);
                }
            } else {
                flat.par_chunks_mut(bins)
                    .enumerate()
                    .for_each(|(i, chunk)| {
                        let params_f32 = CpuAperiodicityParamsF32 {
                            h_factor: h_factor as f32,
                            c_factor: c_factor as f32,
                            breathiness_factor: breathiness_factor as f32,
                            b_scale: b_scale as f32,
                            onset_breath_factor: onset[i],
                        };
                        let is_voiced = vuv_render.get(i).copied().unwrap_or(0) != 0;
                        apply_aperiodicity_frame_f32(chunk, is_voiced, params_f32, true);
                        stabilize_aperiodicity_frame_f32(
                            chunk,
                            is_voiced,
                            onset[i],
                            quality_profile,
                        );
                    });
            }

            for (i, (frame, chunk)) in ap_render
                .iter_mut()
                .zip(flat.chunks_exact(bins))
                .enumerate()
            {
                for (dst, &src) in frame.iter_mut().zip(chunk.iter()) {
                    *dst = src as f64;
                }
                let is_voiced = vuv_render.get(i).copied().unwrap_or(0) != 0;
                stabilize_aperiodicity_frame(
                    frame,
                    is_voiced,
                    onset_breath_factor_at(i, onset_fadein_frames),
                    quality_profile,
                );
            }
            return;
        }
    }

    const PAR_THRESHOLD: usize = 2048;
    if ap_render.len() < PAR_THRESHOLD {
        let mut scratch_f32 = Vec::new();
        for (i, frame) in ap_render.iter_mut().enumerate() {
            let is_voiced = vuv_render.get(i).copied().unwrap_or(0) != 0;
            let params = params_for_frame(
                i,
                onset_fadein_frames,
                h_factor,
                c_factor,
                breathiness_factor,
                b_scale,
            );

            if use_simd && frame.len() >= SIMD_F32_CONVERT_MIN_BINS {
                apply_aperiodicity_frame_simd_f32_with_scratch(
                    frame.as_mut_slice(),
                    is_voiced,
                    params,
                    &mut scratch_f32,
                );
                stabilize_aperiodicity_frame(
                    frame.as_mut_slice(),
                    is_voiced,
                    params.onset_breath_factor,
                    quality_profile,
                );
            } else {
                apply_aperiodicity_frame_scalar(frame.as_mut_slice(), is_voiced, params);
            }
        }
    } else if use_simd {
        ap_render.par_iter_mut().enumerate().for_each_init(
            Vec::new,
            |scratch_f32: &mut Vec<f32>, (i, frame)| {
                let is_voiced = vuv_render.get(i).copied().unwrap_or(0) != 0;
                let params = params_for_frame(
                    i,
                    onset_fadein_frames,
                    h_factor,
                    c_factor,
                    breathiness_factor,
                    b_scale,
                );

                if frame.len() >= SIMD_F32_CONVERT_MIN_BINS {
                    apply_aperiodicity_frame_simd_f32_with_scratch(
                        frame.as_mut_slice(),
                        is_voiced,
                        params,
                        scratch_f32,
                    );
                    stabilize_aperiodicity_frame(
                        frame.as_mut_slice(),
                        is_voiced,
                        params.onset_breath_factor,
                        quality_profile,
                    );
                } else {
                    apply_aperiodicity_frame_scalar(frame.as_mut_slice(), is_voiced, params);
                }
            },
        );
    } else {
        ap_render.par_iter_mut().enumerate().for_each(|(i, frame)| {
            let is_voiced = vuv_render.get(i).copied().unwrap_or(0) != 0;
            let params = params_for_frame(
                i,
                onset_fadein_frames,
                h_factor,
                c_factor,
                breathiness_factor,
                b_scale,
            );

            apply_aperiodicity_frame_scalar(frame.as_mut_slice(), is_voiced, params);
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub fn try_apply_aperiodicity_gpu_batch(
    ap_render: &mut [Vec<f64>],
    vuv_render: &[u8],
    onset_fadein_frames: usize,
    h_factor: f64,
    c_factor: f64,
    breathiness_factor: f64,
    b_scale: f64,
    _quality_profile: QualityProfile,
) -> Result<(), String> {
    #[cfg(feature = "gpu-warp")]
    {
        pollster::block_on(run_wgpu_aperiodicity_batch(
            ap_render,
            vuv_render,
            onset_fadein_frames,
            h_factor as f32,
            c_factor as f32,
            breathiness_factor as f32,
            b_scale as f32,
        ))
    }
    #[cfg(not(feature = "gpu-warp"))]
    {
        let _ = ap_render;
        let _ = vuv_render;
        let _ = onset_fadein_frames;
        let _ = h_factor;
        let _ = c_factor;
        let _ = breathiness_factor;
        let _ = b_scale;
        Err("gpu-warp feature is disabled at build time".to_string())
    }
}

#[cfg(feature = "gpu-warp")]
fn allocate_gpu_buffers(
    device: &wgpu::Device,
    ctx: &GpuWarpContext,
    bins: usize,
    frame_count: usize,
) -> GpuWarpBufferCache {
    let alloc_frames = frame_count.next_power_of_two().max(2048);
    let alloc_total = alloc_frames * bins;

    let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_input"),
        size: (alloc_total * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_output"),
        size: (alloc_total * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let idx_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_idx"),
        size: (bins * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let frac_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_frac"),
        size: (bins * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let clamped_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_clamped"),
        size: (bins * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_params"),
        size: std::mem::size_of::<WarpParams>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("warp_bg"),
        layout: &ctx.warp_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: idx_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: frac_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: clamped_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: params_buffer.as_entire_binding(),
            },
        ],
    });

    GpuWarpBufferCache {
        bins,
        frames: alloc_frames,
        input_buffer,
        output_buffer,
        idx_buffer,
        frac_buffer,
        clamped_buffer,
        params_buffer,
        bind_group,
        host_input_data: Vec::with_capacity(alloc_total),
        host_idx_floor: vec![0; bins],
        host_frac: vec![0.0; bins],
        host_clamped: vec![0; bins],
        lut_uploaded: false,
        chunk_readbacks: Vec::new(),
        chunk_readback_bytes: 0,
    }
}

#[cfg(feature = "gpu-warp")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WarpParams {
    bins: u32,
    frames: u32,
    _pad0: u32,
    _pad1: u32,
}

#[cfg(feature = "gpu-warp")]
struct GpuWarpContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    warp_bind_group_layout: wgpu::BindGroupLayout,
    warp_pipeline: wgpu::ComputePipeline,
    ap_bind_group_layout: wgpu::BindGroupLayout,
    ap_pipeline: wgpu::ComputePipeline,
}

#[cfg(feature = "gpu-warp")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct AperiodicityParams {
    bins: u32,
    frames: u32,
    onset_fadein_frames: u32,
    _pad0: u32,
    h_factor: f32,
    c_factor: f32,
    breathiness_factor: f32,
    b_scale: f32,
}

#[cfg(feature = "gpu-warp")]
static GPU_WARP_CONTEXT: OnceLock<Result<GpuWarpContext, String>> = OnceLock::new();

#[cfg(feature = "gpu-warp")]
const WARP_SHADER: &str = r#"
struct WarpParams {
    bins: u32,
    frames: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<storage, read> input_data: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_data: array<f32>;
@group(0) @binding(2) var<storage, read> idx_floor: array<u32>;
@group(0) @binding(3) var<storage, read> frac: array<f32>;
@group(0) @binding(4) var<storage, read> clamped: array<u32>;
@group(0) @binding(5) var<uniform> params: WarpParams;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let bin = gid.x;
    let frame = gid.y;
    if (bin >= params.bins || frame >= params.frames) {
        return;
    }

    let base = frame * params.bins;
    let i = base + bin;

    if (clamped[bin] != 0u) {
        output_data[i] = input_data[base + params.bins - 1u];
    } else {
        let fl = idx_floor[bin];
        let t = frac[bin];
        output_data[i] = input_data[base + fl] * (1.0 - t) + input_data[base + fl + 1u] * t;
    }
}
"#;

#[cfg(feature = "gpu-warp")]
const APERIODICITY_SHADER: &str = r#"
struct AperiodicityParams {
    bins: u32,
    frames: u32,
    onset_fadein_frames: u32,
    _pad0: u32,
    h_factor: f32,
    c_factor: f32,
    breathiness_factor: f32,
    b_scale: f32,
};

@group(0) @binding(0) var<storage, read> input_data: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_data: array<f32>;
@group(0) @binding(2) var<storage, read> vuv_data: array<u32>;
@group(0) @binding(3) var<uniform> params: AperiodicityParams;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let bin = gid.x;
    let frame = gid.y;
    if (bin >= params.bins || frame >= params.frames) {
        return;
    }

    let idx = frame * params.bins + bin;
    let is_voiced = vuv_data[frame] != 0u;
    var a = input_data[idx];

    if (is_voiced) {
        if (params.h_factor > 0.0) {
            a = a * (1.0 - params.h_factor);
        }
    } else {
        if (params.c_factor > 0.0) {
            a = a * (1.0 - params.c_factor);
        }
    }

    if (params.breathiness_factor > 0.0) {
        a = a + (1.0 - a) * params.breathiness_factor;
    } else if (params.breathiness_factor < 0.0) {
        a = a * params.b_scale;
    }

    if (params.onset_fadein_frames > 0u && frame < params.onset_fadein_frames) {
        let progress = f32(frame) / f32(params.onset_fadein_frames);
        let onset_breath = 1.0 - (1.0 - cos(progress * 3.14159265)) * 0.5;
        a = a + (1.0 - a) * onset_breath;
    }

    output_data[idx] = clamp(a, 0.0, 1.0);
}
"#;

#[cfg(feature = "gpu-warp")]
fn get_or_init_gpu_context() -> Result<&'static GpuWarpContext, String> {
    let result = GPU_WARP_CONTEXT.get_or_init(|| pollster::block_on(init_gpu_context()));
    match result {
        Ok(ctx) => Ok(ctx),
        Err(e) => Err(e.clone()),
    }
}

#[cfg(feature = "gpu-warp")]
async fn init_gpu_context() -> Result<GpuWarpContext, String> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .ok_or_else(|| "no compatible GPU adapter found".to_string())?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .map_err(|e| format!("failed to request device: {e}"))?;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("warp_shader"),
        source: wgpu::ShaderSource::Wgsl(WARP_SHADER.into()),
    });

    let warp_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("warp_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

    let warp_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("warp_pipeline_layout"),
        bind_group_layouts: &[&warp_bind_group_layout],
        push_constant_ranges: &[],
    });

    let warp_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("warp_pipeline"),
        layout: Some(&warp_pipeline_layout),
        module: &shader,
        entry_point: "main",
        compilation_options: wgpu::PipelineCompilationOptions::default(),
    });

    let ap_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("ap_shader"),
        source: wgpu::ShaderSource::Wgsl(APERIODICITY_SHADER.into()),
    });

    let ap_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ap_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let ap_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ap_pipeline_layout"),
        bind_group_layouts: &[&ap_bind_group_layout],
        push_constant_ranges: &[],
    });

    let ap_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("ap_pipeline"),
        layout: Some(&ap_pipeline_layout),
        module: &ap_shader,
        entry_point: "main",
        compilation_options: wgpu::PipelineCompilationOptions::default(),
    });

    Ok(GpuWarpContext {
        device,
        queue,
        warp_bind_group_layout,
        warp_pipeline,
        ap_bind_group_layout,
        ap_pipeline,
    })
}

#[cfg(feature = "gpu-warp")]
struct GpuWarpBufferCache {
    bins: usize,
    frames: usize,
    input_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    idx_buffer: wgpu::Buffer,
    frac_buffer: wgpu::Buffer,
    clamped_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    host_input_data: Vec<f32>,
    host_idx_floor: Vec<u32>,
    host_frac: Vec<f32>,
    host_clamped: Vec<u32>,
    lut_uploaded: bool,
    chunk_readbacks: Vec<wgpu::Buffer>,
    chunk_readback_bytes: u64,
}

#[cfg(feature = "gpu-warp")]
struct GpuAperiodicityBufferCache {
    bins: usize,
    frames: usize,
    input_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    vuv_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    host_input_data: Vec<f32>,
    host_vuv: Vec<u32>,
    chunk_readbacks: Vec<wgpu::Buffer>,
    chunk_readback_bytes: u64,
}

#[cfg(feature = "gpu-warp")]
#[derive(Clone, Copy)]
enum FrameCapacityClass {
    Small,
    Large,
}

#[cfg(feature = "gpu-warp")]
#[derive(Default)]
struct GpuWarpCacheBuckets {
    small: Option<GpuWarpBufferCache>,
    large: Option<GpuWarpBufferCache>,
}

#[cfg(feature = "gpu-warp")]
static GPU_WARP_CACHE: Mutex<GpuWarpCacheBuckets> = Mutex::new(GpuWarpCacheBuckets {
    small: None,
    large: None,
});

#[cfg(feature = "gpu-warp")]
static GPU_AP_CACHE: Mutex<GpuWarpCacheBucketsAp> = Mutex::new(GpuWarpCacheBucketsAp {
    small: None,
    large: None,
});

#[cfg(feature = "gpu-warp")]
struct GpuWarpCacheBucketsAp {
    small: Option<GpuAperiodicityBufferCache>,
    large: Option<GpuAperiodicityBufferCache>,
}

#[cfg(feature = "gpu-warp")]
const SMALL_BUCKET_MAX_FRAMES: usize = 4096;

#[cfg(feature = "gpu-warp")]
fn classify_capacity(frames: usize) -> FrameCapacityClass {
    if frames <= SMALL_BUCKET_MAX_FRAMES {
        FrameCapacityClass::Small
    } else {
        FrameCapacityClass::Large
    }
}

#[cfg(feature = "gpu-warp")]
fn select_bucket_mut(
    buckets: &mut GpuWarpCacheBuckets,
    class: FrameCapacityClass,
) -> &mut Option<GpuWarpBufferCache> {
    match class {
        FrameCapacityClass::Small => &mut buckets.small,
        FrameCapacityClass::Large => &mut buckets.large,
    }
}

#[cfg(feature = "gpu-warp")]
fn select_bucket_mut_ap(
    buckets: &mut GpuWarpCacheBucketsAp,
    class: FrameCapacityClass,
) -> &mut Option<GpuAperiodicityBufferCache> {
    match class {
        FrameCapacityClass::Small => &mut buckets.small,
        FrameCapacityClass::Large => &mut buckets.large,
    }
}

#[cfg(feature = "gpu-warp")]
fn choose_chunk_frames(frame_count: usize) -> usize {
    let default_full_batch = frame_count.max(1);
    let configured = std::env::var("WARP_GPU_CHUNK_FRAMES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default_full_batch)
        .max(1);
    configured.min(frame_count.max(1))
}

#[cfg(feature = "gpu-warp")]
fn choose_inflight_depth() -> usize {
    std::env::var("WARP_GPU_INFLIGHT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 8)
}

#[cfg(feature = "gpu-warp")]
fn ensure_readbacks(
    device: &wgpu::Device,
    readbacks: &mut Vec<wgpu::Buffer>,
    current_bytes_per_slot: &mut u64,
    slots: usize,
    bytes_per_slot: u64,
) {
    if readbacks.len() == slots && *current_bytes_per_slot >= bytes_per_slot {
        return;
    }

    readbacks.clear();
    *current_bytes_per_slot = bytes_per_slot;
    readbacks.extend((0..slots).map(|_| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes_per_slot,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }));
}

#[cfg(feature = "gpu-warp")]
fn return_cache(bufs: GpuWarpBufferCache) {
    let class = classify_capacity(bufs.frames);
    match GPU_WARP_CACHE.lock() {
        Ok(mut cache_guard) => {
            let slot = select_bucket_mut(&mut cache_guard, class);
            *slot = Some(bufs);
        }
        Err(e) => {
            GPU_WARP_CACHE_RETURN_LOCK_FAILURES.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                "Failed to return GPU warp cache due to poisoned mutex: {}",
                e
            );
        }
    }
}

#[cfg(feature = "gpu-warp")]
fn return_ap_cache(bufs: GpuAperiodicityBufferCache) {
    let class = classify_capacity(bufs.frames);
    match GPU_AP_CACHE.lock() {
        Ok(mut cache_guard) => {
            let slot = select_bucket_mut_ap(&mut cache_guard, class);
            *slot = Some(bufs);
        }
        Err(e) => {
            GPU_WARP_CACHE_RETURN_LOCK_FAILURES.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                "Failed to return GPU aperiodicity cache due to poisoned mutex: {}",
                e
            );
        }
    }
}

#[cfg(feature = "gpu-warp")]
fn allocate_aperiodicity_gpu_buffers(
    device: &wgpu::Device,
    ctx: &GpuWarpContext,
    bins: usize,
    frame_count: usize,
) -> GpuAperiodicityBufferCache {
    let alloc_frames = frame_count.next_power_of_two().max(2048);
    let alloc_total = alloc_frames * bins;

    let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ap_input"),
        size: (alloc_total * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ap_output"),
        size: (alloc_total * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let vuv_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ap_vuv"),
        size: (alloc_frames * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ap_params"),
        size: std::mem::size_of::<AperiodicityParams>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ap_bg"),
        layout: &ctx.ap_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: vuv_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: params_buffer.as_entire_binding(),
            },
        ],
    });

    GpuAperiodicityBufferCache {
        bins,
        frames: alloc_frames,
        input_buffer,
        output_buffer,
        vuv_buffer,
        params_buffer,
        bind_group,
        host_input_data: Vec::with_capacity(alloc_total),
        host_vuv: vec![0; alloc_frames],
        chunk_readbacks: Vec::new(),
        chunk_readback_bytes: 0,
    }
}

#[cfg(feature = "gpu-warp")]
async fn run_wgpu_warp_batch(frames: &mut [Vec<f64>], lut: &WarpLut) -> Result<(), String> {
    if frames.is_empty() {
        return Ok(());
    }

    let bins = lut.idx_floor.len();
    if bins == 0 {
        return Ok(());
    }

    if frames.iter().any(|f| f.len() != bins) {
        return Err("inconsistent spectrum frame length for gpu warp".to_string());
    }

    let frame_count = frames.len();
    let ctx = get_or_init_gpu_context()?;
    let device = &ctx.device;
    let queue = &ctx.queue;

    let chunk_frames = choose_chunk_frames(frame_count);
    let required_capacity = chunk_frames.min(frame_count);
    let bucket_class = classify_capacity(required_capacity);

    let mut bufs = {
        let mut cache_guard = GPU_WARP_CACHE
            .lock()
            .map_err(|e| format!("cache mutex poisoned: {}", e))?;
        let slot = select_bucket_mut(&mut cache_guard, bucket_class);
        if let Some(b) = slot.take() {
            if b.bins == bins && b.frames >= required_capacity {
                GPU_WARP_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                b
            } else {
                GPU_WARP_CACHE_REALLOCS.fetch_add(1, Ordering::Relaxed);
                GPU_WARP_BUFFER_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
                allocate_gpu_buffers(device, ctx, bins, required_capacity)
            }
        } else {
            GPU_WARP_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
            GPU_WARP_BUFFER_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            allocate_gpu_buffers(device, ctx, bins, required_capacity)
        }
    };

    let mut lut_changed = !bufs.lut_uploaded;
    for i in 0..bins {
        let idx_u32 = lut.idx_floor[i] as u32;
        let frac_f32 = lut.frac[i];
        let clamped_u32 = u32::from(lut.clamped[i]);

        if bufs.host_idx_floor[i] != idx_u32
            || bufs.host_frac[i] != frac_f32
            || bufs.host_clamped[i] != clamped_u32
        {
            bufs.host_idx_floor[i] = idx_u32;
            bufs.host_frac[i] = frac_f32;
            bufs.host_clamped[i] = clamped_u32;
            lut_changed = true;
        }
    }

    if lut_changed {
        queue.write_buffer(
            &bufs.idx_buffer,
            0,
            bytemuck::cast_slice(&bufs.host_idx_floor),
        );
        queue.write_buffer(&bufs.frac_buffer, 0, bytemuck::cast_slice(&bufs.host_frac));
        queue.write_buffer(
            &bufs.clamped_buffer,
            0,
            bytemuck::cast_slice(&bufs.host_clamped),
        );
        GPU_WARP_LUT_UPLOADS.fetch_add(1, Ordering::Relaxed);
        bufs.lut_uploaded = true;
    }

    let inflight = choose_inflight_depth();
    ensure_readbacks(
        device,
        &mut bufs.chunk_readbacks,
        &mut bufs.chunk_readback_bytes,
        inflight,
        (chunk_frames * bins * std::mem::size_of::<f32>()) as u64,
    );

    struct WarpPending {
        chunk_start: usize,
        chunk_len: usize,
        readback_idx: usize,
        expected_len: usize,
        rx: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    }

    let mut pending: VecDeque<WarpPending> = VecDeque::new();
    let mut submit_index: usize = 0;

    let drain_one = |pending: &mut VecDeque<WarpPending>| -> Result<WarpPending, String> {
        if let Some(p) = pending.pop_front() {
            device.poll(wgpu::Maintain::Wait);
            let map_result = match p.rx.recv() {
                Ok(res) => res,
                Err(e) => return Err(format!("gpu map channel failed: {e}")),
            };
            if let Err(e) = map_result {
                return Err(format!("failed to map readback buffer: {e:?}"));
            }
            Ok(p)
        } else {
            Err("internal pending queue underflow".to_string())
        }
    };

    for chunk_start in (0..frame_count).step_by(chunk_frames) {
        let chunk_end = (chunk_start + chunk_frames).min(frame_count);
        let chunk_len = chunk_end - chunk_start;
        let chunk_total = chunk_len * bins;
        let readback_idx = submit_index % inflight;
        submit_index = submit_index.wrapping_add(1);

        bufs.host_input_data.clear();
        for frame in frames[chunk_start..chunk_end].iter() {
            bufs.host_input_data.extend(frame.iter().map(|&v| v as f32));
        }

        let input_bytes = bytemuck::cast_slice(&bufs.host_input_data);
        queue.write_buffer(&bufs.input_buffer, 0, input_bytes);

        let params = WarpParams {
            bins: bins as u32,
            frames: chunk_len as u32,
            _pad0: 0,
            _pad1: 0,
        };
        queue.write_buffer(&bufs.params_buffer, 0, bytemuck::bytes_of(&params));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("warp_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("warp_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&ctx.warp_pipeline);
            pass.set_bind_group(0, &bufs.bind_group, &[]);
            let workgroup_size = 256u32;
            let groups_x = (bins as u32).div_ceil(workgroup_size);
            let groups_y = chunk_len as u32;
            pass.dispatch_workgroups(groups_x, groups_y, 1);
        }

        encoder.copy_buffer_to_buffer(
            &bufs.output_buffer,
            0,
            &bufs.chunk_readbacks[readback_idx],
            0,
            input_bytes.len() as u64,
        );
        queue.submit(Some(encoder.finish()));
        GPU_WARP_CHUNK_DISPATCHES.fetch_add(1, Ordering::Relaxed);

        let slice = bufs.chunk_readbacks[readback_idx].slice(0..input_bytes.len() as u64);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(
            wgpu::MapMode::Read,
            move |res: Result<(), wgpu::BufferAsyncError>| {
                let _ = tx.send(res);
            },
        );

        pending.push_back(WarpPending {
            chunk_start,
            chunk_len,
            readback_idx,
            expected_len: chunk_total,
            rx,
        });

        if pending.len() >= inflight {
            let p = match drain_one(&mut pending) {
                Ok(p) => p,
                Err(e) => {
                    GPU_WARP_MAP_ERRORS.fetch_add(1, Ordering::Relaxed);
                    return_cache(bufs);
                    return Err(e);
                }
            };

            let p_slice = bufs.chunk_readbacks[p.readback_idx]
                .slice(0..(p.expected_len * std::mem::size_of::<f32>()) as u64);
            {
                let mapped = p_slice.get_mapped_range();
                let output: &[f32] = bytemuck::cast_slice(&mapped);
                if output.len() != p.expected_len {
                    drop(mapped);
                    bufs.chunk_readbacks[p.readback_idx].unmap();
                    return_cache(bufs);
                    return Err("gpu output size mismatch".to_string());
                }
                let start = p.chunk_start;
                let end = p.chunk_start + p.chunk_len;
                frames[start..end]
                    .iter_mut()
                    .zip(output.chunks_exact(bins))
                    .for_each(|(frame, chunk)| {
                        for i in 0..bins {
                            frame[i] = chunk[i] as f64;
                        }
                    });
            }
            bufs.chunk_readbacks[p.readback_idx].unmap();
        }
    }

    while let Some(p) = {
        if pending.is_empty() {
            None
        } else {
            match drain_one(&mut pending) {
                Ok(v) => Some(v),
                Err(e) => {
                    GPU_WARP_MAP_ERRORS.fetch_add(1, Ordering::Relaxed);
                    return_cache(bufs);
                    return Err(e);
                }
            }
        }
    } {
        let p_slice = bufs.chunk_readbacks[p.readback_idx]
            .slice(0..(p.expected_len * std::mem::size_of::<f32>()) as u64);
        {
            let mapped = p_slice.get_mapped_range();
            let output: &[f32] = bytemuck::cast_slice(&mapped);
            if output.len() != p.expected_len {
                drop(mapped);
                bufs.chunk_readbacks[p.readback_idx].unmap();
                return_cache(bufs);
                return Err("gpu output size mismatch".to_string());
            }
            let start = p.chunk_start;
            let end = p.chunk_start + p.chunk_len;
            frames[start..end]
                .iter_mut()
                .zip(output.chunks_exact(bins))
                .for_each(|(frame, chunk)| {
                    for i in 0..bins {
                        frame[i] = chunk[i] as f64;
                    }
                });
        }
        bufs.chunk_readbacks[p.readback_idx].unmap();
    }

    return_cache(bufs);
    Ok(())
}

#[cfg(feature = "gpu-warp")]
async fn run_wgpu_aperiodicity_batch(
    ap_render: &mut [Vec<f64>],
    vuv_render: &[u8],
    onset_fadein_frames: usize,
    h_factor: f32,
    c_factor: f32,
    breathiness_factor: f32,
    b_scale: f32,
) -> Result<(), String> {
    if ap_render.is_empty() {
        return Ok(());
    }

    let bins = ap_render[0].len();
    if bins == 0 {
        return Ok(());
    }

    if ap_render.iter().any(|f| f.len() != bins) {
        return Err("inconsistent aperiodicity frame length for gpu path".to_string());
    }
    if vuv_render.len() < ap_render.len() {
        return Err("vuv_render length is smaller than ap_render length".to_string());
    }

    let frame_count = ap_render.len();
    let ctx = get_or_init_gpu_context()?;
    let device = &ctx.device;
    let queue = &ctx.queue;

    let chunk_frames = choose_chunk_frames(frame_count);
    let required_capacity = chunk_frames.min(frame_count);
    let bucket_class = classify_capacity(required_capacity);

    let mut bufs = {
        let mut cache_guard = GPU_AP_CACHE
            .lock()
            .map_err(|e| format!("ap cache mutex poisoned: {}", e))?;
        let slot = select_bucket_mut_ap(&mut cache_guard, bucket_class);
        if let Some(b) = slot.take() {
            if b.bins == bins && b.frames >= required_capacity {
                GPU_WARP_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                b
            } else {
                GPU_WARP_CACHE_REALLOCS.fetch_add(1, Ordering::Relaxed);
                GPU_WARP_BUFFER_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
                allocate_aperiodicity_gpu_buffers(device, ctx, bins, required_capacity)
            }
        } else {
            GPU_WARP_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
            GPU_WARP_BUFFER_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            allocate_aperiodicity_gpu_buffers(device, ctx, bins, required_capacity)
        }
    };

    let inflight = choose_inflight_depth();
    ensure_readbacks(
        device,
        &mut bufs.chunk_readbacks,
        &mut bufs.chunk_readback_bytes,
        inflight,
        (chunk_frames * bins * std::mem::size_of::<f32>()) as u64,
    );

    struct ApPending {
        chunk_start: usize,
        chunk_len: usize,
        readback_idx: usize,
        expected_len: usize,
        rx: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    }

    let mut pending: VecDeque<ApPending> = VecDeque::new();
    let mut submit_index: usize = 0;

    let drain_one = |pending: &mut VecDeque<ApPending>| -> Result<ApPending, String> {
        if let Some(p) = pending.pop_front() {
            device.poll(wgpu::Maintain::Wait);
            let map_result = match p.rx.recv() {
                Ok(res) => res,
                Err(e) => return Err(format!("gpu ap map channel failed: {e}")),
            };
            if let Err(e) = map_result {
                return Err(format!("failed to map ap readback buffer: {e:?}"));
            }
            Ok(p)
        } else {
            Err("internal ap pending queue underflow".to_string())
        }
    };

    for chunk_start in (0..frame_count).step_by(chunk_frames) {
        let chunk_end = (chunk_start + chunk_frames).min(frame_count);
        let chunk_len = chunk_end - chunk_start;
        let chunk_total = chunk_len * bins;
        let readback_idx = submit_index % inflight;
        submit_index = submit_index.wrapping_add(1);

        bufs.host_input_data.clear();
        for frame in ap_render[chunk_start..chunk_end].iter() {
            bufs.host_input_data.extend(frame.iter().map(|&v| v as f32));
        }

        for (i, &is_voiced) in vuv_render[chunk_start..chunk_end].iter().enumerate() {
            bufs.host_vuv[i] = is_voiced as u32;
        }

        let input_bytes = bytemuck::cast_slice(&bufs.host_input_data);
        queue.write_buffer(&bufs.input_buffer, 0, input_bytes);
        queue.write_buffer(
            &bufs.vuv_buffer,
            0,
            bytemuck::cast_slice(&bufs.host_vuv[..chunk_len]),
        );

        let params = AperiodicityParams {
            bins: bins as u32,
            frames: chunk_len as u32,
            onset_fadein_frames: onset_fadein_frames as u32,
            _pad0: 0,
            h_factor,
            c_factor,
            breathiness_factor,
            b_scale,
        };
        queue.write_buffer(&bufs.params_buffer, 0, bytemuck::bytes_of(&params));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ap_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ap_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&ctx.ap_pipeline);
            pass.set_bind_group(0, &bufs.bind_group, &[]);
            let workgroup_size = 256u32;
            let groups_x = (bins as u32).div_ceil(workgroup_size);
            let groups_y = chunk_len as u32;
            pass.dispatch_workgroups(groups_x, groups_y, 1);
        }

        encoder.copy_buffer_to_buffer(
            &bufs.output_buffer,
            0,
            &bufs.chunk_readbacks[readback_idx],
            0,
            input_bytes.len() as u64,
        );
        queue.submit(Some(encoder.finish()));
        GPU_WARP_CHUNK_DISPATCHES.fetch_add(1, Ordering::Relaxed);

        let slice = bufs.chunk_readbacks[readback_idx].slice(0..input_bytes.len() as u64);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(
            wgpu::MapMode::Read,
            move |res: Result<(), wgpu::BufferAsyncError>| {
                let _ = tx.send(res);
            },
        );

        pending.push_back(ApPending {
            chunk_start,
            chunk_len,
            readback_idx,
            expected_len: chunk_total,
            rx,
        });

        if pending.len() >= inflight {
            let p = match drain_one(&mut pending) {
                Ok(p) => p,
                Err(e) => {
                    GPU_WARP_MAP_ERRORS.fetch_add(1, Ordering::Relaxed);
                    return_ap_cache(bufs);
                    return Err(e);
                }
            };

            let p_slice = bufs.chunk_readbacks[p.readback_idx]
                .slice(0..(p.expected_len * std::mem::size_of::<f32>()) as u64);
            {
                let mapped = p_slice.get_mapped_range();
                let output: &[f32] = bytemuck::cast_slice(&mapped);
                if output.len() != p.expected_len {
                    drop(mapped);
                    bufs.chunk_readbacks[p.readback_idx].unmap();
                    return_ap_cache(bufs);
                    return Err("gpu ap output size mismatch".to_string());
                }

                let start = p.chunk_start;
                let end = p.chunk_start + p.chunk_len;
                ap_render[start..end]
                    .iter_mut()
                    .zip(output.chunks_exact(bins))
                    .for_each(|(frame, chunk)| {
                        for i in 0..bins {
                            frame[i] = chunk[i] as f64;
                        }
                    });
            }
            bufs.chunk_readbacks[p.readback_idx].unmap();
        }
    }

    while let Some(p) = {
        if pending.is_empty() {
            None
        } else {
            match drain_one(&mut pending) {
                Ok(v) => Some(v),
                Err(e) => {
                    GPU_WARP_MAP_ERRORS.fetch_add(1, Ordering::Relaxed);
                    return_ap_cache(bufs);
                    return Err(e);
                }
            }
        }
    } {
        let p_slice = bufs.chunk_readbacks[p.readback_idx]
            .slice(0..(p.expected_len * std::mem::size_of::<f32>()) as u64);
        {
            let mapped = p_slice.get_mapped_range();
            let output: &[f32] = bytemuck::cast_slice(&mapped);
            if output.len() != p.expected_len {
                drop(mapped);
                bufs.chunk_readbacks[p.readback_idx].unmap();
                return_ap_cache(bufs);
                return Err("gpu ap output size mismatch".to_string());
            }

            let start = p.chunk_start;
            let end = p.chunk_start + p.chunk_len;
            ap_render[start..end]
                .iter_mut()
                .zip(output.chunks_exact(bins))
                .for_each(|(frame, chunk)| {
                    for i in 0..bins {
                        frame[i] = chunk[i] as f64;
                    }
                });
        }
        bufs.chunk_readbacks[p.readback_idx].unmap();
    }

    return_ap_cache(bufs);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(len: usize, seed: f64) -> Vec<f64> {
        (0..len)
            .map(|i| {
                let x = (i as f64 * 0.17 + seed).sin() * 0.5 + 0.5;
                x.clamp(0.0, 1.0)
            })
            .collect()
    }

    fn make_frame_f32(len: usize, seed: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let x = (i as f32 * 0.17 + seed).sin() * 0.5 + 0.5;
                x.clamp(0.0, 1.0)
            })
            .collect()
    }

    #[test]
    fn aperiodicity_scalar_output_stays_finite_and_clamped() {
        let mut frame = make_frame(257, 0.3);
        apply_aperiodicity_frame_scalar(
            &mut frame,
            true,
            CpuAperiodicityParams {
                h_factor: 0.2,
                c_factor: 0.25,
                breathiness_factor: 0.1,
                b_scale: 0.8,
                onset_breath_factor: 0.4,
            },
        );

        assert!(frame.iter().all(|v| v.is_finite()));
        assert!(frame.iter().all(|v| *v >= 0.0 && *v <= 1.0));
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn aperiodicity_avx2_matches_scalar() {
        if !cpu_supports_ap_simd() {
            return;
        }

        let mut scalar = make_frame_f32(1027, 0.9);
        let mut simd = scalar.clone();

        let params = CpuAperiodicityParamsF32 {
            h_factor: 0.2,
            c_factor: 0.25,
            breathiness_factor: -0.12,
            b_scale: 0.8,
            onset_breath_factor: 0.37,
        };

        apply_aperiodicity_frame_scalar_f32(&mut scalar, false, params);
        unsafe {
            apply_aperiodicity_frame_avx2_f32(&mut simd, false, params);
        }

        for (a, b) in scalar.iter().zip(simd.iter()) {
            assert!((a - b).abs() <= 1e-6, "scalar={a}, simd={b}");
        }
    }

    #[test]
    fn aperiodicity_stabilizer_reduces_unvoiced_high_band_spikes() {
        let mut frame = vec![0.02; 32];
        frame[28] = 1.0;
        frame[29] = 0.98;
        frame[30] = 0.12;

        let profile = QualityProfile::from_preset(crate::config::QualityPreset::Balanced);
        stabilize_aperiodicity_frame(&mut frame, false, 0.8, profile);

        assert!(frame[28] < 0.95);
        assert!(frame[29] < 0.95);
        assert!(frame[0] >= 0.04);
    }

    #[test]
    fn aperiodicity_stabilizer_caps_voiced_high_band() {
        let mut frame = vec![0.4; 32];
        for v in &mut frame[24..] {
            *v = 1.0;
        }

        let profile = QualityProfile::from_preset(crate::config::QualityPreset::Balanced);
        stabilize_aperiodicity_frame(&mut frame, true, 0.7, profile);

        assert!(frame[24..].iter().all(|v| *v < 0.95));
    }

    #[test]
    fn classic_profile_preserves_legacy_behavior_knobs() {
        let classic = QualityProfile::from_preset(crate::config::QualityPreset::Classic);
        assert_eq!(classic.high_pitch_warp_relax, 0.0);
        assert_eq!(classic.presence_max_gain, 0.0);
        assert_eq!(classic.unvoiced_spike_blend, 0.0);
    }
}
