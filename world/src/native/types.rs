use super::constants::*;
pub use crate::common::{MatrixF32, MatrixViewF32};

#[derive(Clone, Debug, PartialEq)]
pub struct AcousticFeatures {
    pub temporal_positions: Vec<f32>,
    pub f0: Vec<f32>,
    pub spectrogram: MatrixF32,
    pub aperiodicity: MatrixF32,
    pub frame_period: f32,
    pub fs: i32,
    pub fft_size: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalyzerWorkspace {
    pub(super) refined_f0: Vec<f32>,
    pub(super) stonemask_real_w: Vec<f32>,
    pub(super) stonemask_imag_w: Vec<f32>,
    pub(super) stonemask_real_dw: Vec<f32>,
    pub(super) stonemask_imag_dw: Vec<f32>,
}

impl AnalyzerWorkspace {
    pub fn new() -> Self {
        Self {
            refined_f0: Vec::new(),
            stonemask_real_w: Vec::new(),
            stonemask_imag_w: Vec::new(),
            stonemask_real_dw: Vec::new(),
            stonemask_imag_dw: Vec::new(),
        }
    }
}

impl Default for AnalyzerWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl AcousticFeatures {
    pub fn new(frame_period: f32, fs: i32) -> Self {
        Self {
            temporal_positions: Vec::new(),
            f0: Vec::new(),
            spectrogram: MatrixF32::zeros(0, 0),
            aperiodicity: MatrixF32::zeros(0, 0),
            frame_period,
            fs,
            fft_size: 0,
        }
    }

    pub fn frame_count(&self) -> usize {
        self.f0.len()
    }

    pub fn bin_count(&self) -> usize {
        self.spectrogram.cols()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcousticConfig {
    pub f0_estimation: F0EstimationConfig,
    pub spectral_analysis: SpectralAnalysisConfig,
    pub aperiodicity_analysis: AperiodicityAnalysisConfig,
    pub refine_f0: bool,
}

impl AcousticConfig {
    pub fn new(fs: i32) -> Self {
        Self {
            f0_estimation: F0EstimationConfig::default(),
            spectral_analysis: SpectralAnalysisConfig::new(fs),
            aperiodicity_analysis: AperiodicityAnalysisConfig::default(),
            refine_f0: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpectralAnalysisConfig {
    pub q1: f32,
    pub f0_floor: f32,
    pub fft_size: i32,
}

impl SpectralAnalysisConfig {
    pub fn new(fs: i32) -> Self {
        let mut option = Self {
            q1: CHEAPTRICK_Q1,
            f0_floor: FLOOR_F0,
            fft_size: 0,
        };
        let target = (3.0 * fs as f32 / option.f0_floor.max(SAFE_GUARD_MINIMUM)).ceil() as usize;
        option.fft_size = target.next_power_of_two().max(2) as i32;
        option
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct F0EstimationConfig {
    pub f0_floor: f32,
    pub f0_ceil: f32,
    pub channels_in_octave: f32,
    pub frame_period: f32,
    pub speed: i32,
    pub allowed_range: f32,
}

impl Default for F0EstimationConfig {
    fn default() -> Self {
        Self {
            f0_floor: FLOOR_F0,
            f0_ceil: CEIL_F0,
            channels_in_octave: 2.0,
            frame_period: 5.0,
            speed: 1,
            allowed_range: 0.1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AperiodicityAnalysisConfig {
    pub threshold: f32,
}

impl Default for AperiodicityAnalysisConfig {
    fn default() -> Self {
        Self {
            threshold: D4C_THRESHOLD,
        }
    }
}
