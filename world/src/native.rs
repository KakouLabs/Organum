//! # Native WORLD implementation
//!
//! This module provides a native Rust implementation of the WORLD vocoder algorithms.
//! Native synthesis and codec behavior are compared against C WORLD in the test
//! suite, but the analysis stages are still experimental and should not be
//! treated as a production-equivalent replacement for C WORLD yet.
//!
//! ## Function Mapping Table
//!
//! | C WORLD Function | Rust Target Function | File |
//! | :--- | :--- | :--- |
//! | `GetWindowedWaveform` | `get_windowed_waveform` | `cheaptrick.cpp` |
//! | `CheapTrick` | `cheaptrick_into` | `cheaptrick.cpp` |
//! | `GetPowerSpectrum` | `get_power_spectrum` | `cheaptrick.cpp` |
//! | `DCCorrection` | `dc_correction` | `cheaptrick.cpp` |
//! | `LinearSmoothing` | `linear_smoothing` | `cheaptrick.cpp` |
//! | `SmoothingWithRecovery` | `smoothing_with_recovery` | `cheaptrick.cpp` |
//! | `D4C` | `d4c_into` | `d4c.cpp` |
//! | `Synthesis` | `synthesis_in_place` | `synthesis.cpp` |
//! | `MinimumPhase` | `get_minimum_phase_spectrum`| `synthesis.cpp` |
//! | `StoneMask` | `stonemask_into` | `stonemask.cpp` |
//! | `Dio` | `dio_into` | `dio.cpp` |
//! | `Harvest` | `harvest_into` | `harvest.cpp` |

#![allow(dead_code)]

mod analysis;
mod cheaptrick;
mod codec;
mod constants;
mod d4c;
mod fft;
mod random;
mod synthesis;
mod types;

pub use analysis::{
    analyze, analyze_into, dio, dio_into, get_fft_size_for_cheaptrick,
    get_number_of_aperiodicities, get_samples_for_dio, get_samples_for_harvest, harvest,
    harvest_into, stonemask, stonemask_into,
};
pub use cheaptrick::{
    cheaptrick, cheaptrick_from_spectrum, cheaptrick_from_spectrum_borrowed, cheaptrick_into,
};
pub use codec::{
    code_aperiodicity, code_aperiodicity_into, code_spectral_envelope, code_spectral_envelope_into,
    decode_aperiodicity, decode_aperiodicity_into, decode_spectral_envelope,
    decode_spectral_envelope_into,
};
use constants::*;
pub use d4c::{d4c, d4c_from_spectrum, d4c_into};
pub use synthesis::{synthesis, synthesis_in_place, synthesis_into};
pub use types::*;

/// High-level acoustic analysis pipeline.
pub struct AcousticAnalyzer {
    config: AcousticConfig,
    workspace: AnalyzerWorkspace,
}

impl AcousticAnalyzer {
    pub fn new(fs: i32) -> Self {
        Self {
            config: AcousticConfig::new(fs),
            workspace: AnalyzerWorkspace::new(),
        }
    }

    pub fn with_config(config: AcousticConfig) -> Self {
        Self {
            config,
            workspace: AnalyzerWorkspace::new(),
        }
    }

    pub fn config(&self) -> &AcousticConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut AcousticConfig {
        &mut self.config
    }

    pub fn extract_features(&mut self, x: &[f32], fs: i32) -> AcousticFeatures {
        let mut features = AcousticFeatures::new(self.config.f0_estimation.frame_period, fs);
        self.extract_features_into(x, fs, &mut features);
        features
    }

    pub fn extract_features_into(&mut self, x: &[f32], fs: i32, features: &mut AcousticFeatures) {
        analyze_into(x, fs, &mut self.config, features, &mut self.workspace);
    }
}

/// High-level acoustic synthesis pipeline.
pub struct AcousticSynthesizer;

impl AcousticSynthesizer {
    pub fn new() -> Self {
        Self
    }

    pub fn synthesize(
        &self,
        f0: &[f32],
        spectrogram: &MatrixF32,
        aperiodicity: &MatrixF32,
        frame_period: f32,
        fs: i32,
    ) -> Vec<f32> {
        synthesis(f0, spectrogram, aperiodicity, frame_period, fs)
    }

    pub fn synthesize_into(
        &self,
        f0: &[f32],
        spectrogram: &MatrixF32,
        aperiodicity: &MatrixF32,
        frame_period: f32,
        fs: i32,
        output: &mut Vec<f32>,
    ) {
        synthesis_into(f0, spectrogram, aperiodicity, frame_period, fs, output);
    }
}

#[cfg(test)]
mod tests;
