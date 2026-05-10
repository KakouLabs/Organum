use std::f32::consts::PI;

use super::fft::{fft, ifft};
use super::{
    cheaptrick_into, d4c_into, AcousticConfig, AcousticFeatures, AnalyzerWorkspace,
    F0EstimationConfig, SpectralAnalysisConfig, FREQUENCY_INTERVAL, SAFE_GUARD_MINIMUM,
    UPPER_LIMIT,
};

pub fn get_samples_for_dio(fs: i32, x_length: usize, frame_period: f32) -> usize {
    get_frame_count(fs, x_length, frame_period)
}

pub fn get_samples_for_harvest(fs: i32, x_length: usize, frame_period: f32) -> usize {
    get_frame_count(fs, x_length, frame_period)
}

pub fn get_fft_size_for_cheaptrick(fs: i32, config: &SpectralAnalysisConfig) -> i32 {
    let target = (3.0 * fs as f32 / config.f0_floor.max(SAFE_GUARD_MINIMUM)).ceil() as usize;
    target.next_power_of_two().max(2) as i32
}

pub(super) fn get_f0_floor_for_cheaptrick(fs: i32, fft_size: usize) -> f32 {
    3.0 * fs as f32 / (fft_size as f32 - 3.0)
}

pub fn get_number_of_aperiodicities(fs: i32) -> i32 {
    let upper = (fs as f32 / 2.0 - FREQUENCY_INTERVAL).min(UPPER_LIMIT);
    (upper / FREQUENCY_INTERVAL).floor().max(1.0) as i32
}

pub fn dio(x: &[f32], fs: i32, config: &F0EstimationConfig) -> (Vec<f32>, Vec<f32>) {
    let mut temporal_positions = Vec::new();
    let mut f0 = Vec::new();
    dio_into(x, fs, config, &mut temporal_positions, &mut f0);
    (temporal_positions, f0)
}

pub fn harvest(x: &[f32], fs: i32, config: &F0EstimationConfig) -> (Vec<f32>, Vec<f32>) {
    let mut temporal_positions = Vec::new();
    let mut f0 = Vec::new();
    harvest_into(x, fs, config, &mut temporal_positions, &mut f0);
    (temporal_positions, f0)
}

pub fn dio_into(
    x: &[f32],
    fs: i32,
    config: &F0EstimationConfig,
    temporal_positions: &mut Vec<f32>,
    f0: &mut Vec<f32>,
) {
    let frames = get_frame_count(fs, x.len(), config.frame_period);
    temporal_positions.clear();
    for i in 0..frames {
        temporal_positions.push(i as f32 * config.frame_period / 1000.0);
    }

    let mut candidates = vec![Vec::new(); frames];
    let fft_size = (x.len() * 2).next_power_of_two().max(2048);

    let n_bands =
        (config.channels_in_octave * (config.f0_ceil / config.f0_floor).log2()).ceil() as usize;
    let mut filtered = vec![0.0; fft_size];
    let mut filter_imag = vec![0.0; fft_size];
    for i in 0..n_bands {
        let cutoff = config.f0_floor * 2.0f32.powf((i as f32 + 1.0) / config.channels_in_octave);
        low_pass_filter_into(x, fs, cutoff, fft_size, &mut filtered, &mut filter_imag);
        get_f0_candidates_into(
            x,
            &filtered,
            fs,
            config.f0_floor,
            config.f0_ceil,
            temporal_positions,
            &mut candidates,
        );
    }

    // Selection and contour fixing
    f0.clear();
    f0.resize(frames, 0.0);
    fix_contour_into(&candidates, f0);
    correct_dio_subharmonics_and_edges(f0, config);
}

pub fn harvest_into(
    x: &[f32],
    fs: i32,
    config: &F0EstimationConfig,
    temporal_positions: &mut Vec<f32>,
    f0: &mut Vec<f32>,
) {
    let frames = get_frame_count(fs, x.len(), config.frame_period);
    temporal_positions.clear();
    for i in 0..frames {
        temporal_positions.push(i as f32 * config.frame_period / 1000.0);
    }

    let mut candidates = vec![Vec::new(); frames];
    let fft_size = (x.len() * 2).next_power_of_two().max(2048);

    // Harvest multi-resolution filtering
    let n_resolutions = 6;
    let mut filtered = vec![0.0; fft_size];
    let mut filter_imag = vec![0.0; fft_size];
    for i in 0..n_resolutions {
        let f_center = config.f0_floor * 2.0f32.powf(i as f32);
        if f_center > config.f0_ceil {
            break;
        }

        let f_low = f_center * 0.7;
        let f_high = f_center * 1.3;
        band_pass_filter_into(
            x,
            fs,
            f_low,
            f_high,
            fft_size,
            &mut filtered,
            &mut filter_imag,
        );
        get_harvest_candidates_into(
            x,
            &filtered,
            fs,
            config.f0_floor,
            config.f0_ceil,
            temporal_positions,
            &mut candidates,
        );
    }

    // Path selection and contour fixing
    f0.clear();
    f0.resize(frames, 0.0);
    fix_contour_into(&candidates, f0);

    // Basic median smoothing (3-point)
    if frames >= 3 {
        for i in 1..frames - 1 {
            let mut neighbors = [f0[i - 1], f0[i], f0[i + 1]];
            neighbors.sort_by(|a, b| a.partial_cmp(b).unwrap());
            f0[i] = neighbors[1];
        }
    }
}

pub fn stonemask(x: &[f32], fs: i32, temporal_positions: &[f32], f0: &[f32]) -> Vec<f32> {
    assert_eq!(temporal_positions.len(), f0.len());
    let mut refined_f0 = Vec::with_capacity(f0.len());
    stonemask_into(x, fs, temporal_positions, f0, &mut refined_f0);
    refined_f0
}

fn get_instantaneous_frequency(
    x: &[f32],
    fs: i32,
    temporal_position: f32,
    f0: f32,
    fft_size: usize,
) -> f32 {
    let mut real_w = vec![0.0; fft_size];
    let mut imag_w = vec![0.0; fft_size];
    let mut real_dw = vec![0.0; fft_size];
    let mut imag_dw = vec![0.0; fft_size];

    get_instantaneous_frequency_with_workspace(
        x,
        fs,
        temporal_position,
        f0,
        fft_size,
        &mut real_w,
        &mut imag_w,
        &mut real_dw,
        &mut imag_dw,
    )
}

fn get_instantaneous_frequency_with_workspace(
    x: &[f32],
    fs: i32,
    temporal_position: f32,
    f0: f32,
    fft_size: usize,
    real_w: &mut [f32],
    imag_w: &mut [f32],
    real_dw: &mut [f32],
    imag_dw: &mut [f32],
) -> f32 {
    debug_assert_eq!(real_w.len(), fft_size);
    debug_assert_eq!(imag_w.len(), fft_size);
    debug_assert_eq!(real_dw.len(), fft_size);
    debug_assert_eq!(imag_dw.len(), fft_size);
    real_w.fill(0.0);
    imag_w.fill(0.0);
    real_dw.fill(0.0);
    imag_dw.fill(0.0);

    let half_window_length = (fs as f32 / f0).round() as isize;
    let base_index = (temporal_position * fs as f32).round() as isize;

    for i in 0..fft_size {
        let index = i as isize - fft_size as isize / 2 + base_index;
        if index >= 0 && index < x.len() as isize {
            let pos = (i as f32 - fft_size as f32 / 2.0) / half_window_length as f32;
            if pos.abs() < 1.0 {
                let window = 0.5 + 0.5 * (PI * pos).cos();
                let d_window = -0.5 * PI * (PI * pos).sin() / half_window_length as f32;
                real_w[i] = x[index as usize] * window;
                real_dw[i] = x[index as usize] * d_window;
            }
        }
    }

    fft(real_w, imag_w);
    fft(real_dw, imag_dw);

    let k = (f0 * fft_size as f32 / fs as f32).round() as usize;
    if k >= fft_size / 2 {
        return f0;
    }

    let denom = real_w[k] * real_w[k] + imag_w[k] * imag_w[k];
    if denom <= SAFE_GUARD_MINIMUM {
        return f0;
    }

    let if_raw = (real_w[k] * imag_dw[k] - imag_w[k] * real_dw[k]) / denom;
    let refined_f0 = k as f32 * fs as f32 / fft_size as f32 - if_raw * fs as f32 / (2.0 * PI);

    refined_f0.max(40.0).min(fs as f32 / 2.0)
}

pub fn stonemask_into(
    x: &[f32],
    fs: i32,
    temporal_positions: &[f32],
    f0: &[f32],
    refined_f0: &mut Vec<f32>,
) {
    assert_eq!(temporal_positions.len(), f0.len());
    refined_f0.clear();
    let fft_size = 2048;
    let mut real_w = vec![0.0; fft_size];
    let mut imag_w = vec![0.0; fft_size];
    let mut real_dw = vec![0.0; fft_size];
    let mut imag_dw = vec![0.0; fft_size];

    stonemask_into_with_workspace(
        x,
        fs,
        temporal_positions,
        f0,
        refined_f0,
        &mut real_w,
        &mut imag_w,
        &mut real_dw,
        &mut imag_dw,
    );
}

fn stonemask_into_with_workspace(
    x: &[f32],
    fs: i32,
    temporal_positions: &[f32],
    f0: &[f32],
    refined_f0: &mut Vec<f32>,
    real_w: &mut [f32],
    imag_w: &mut [f32],
    real_dw: &mut [f32],
    imag_dw: &mut [f32],
) {
    assert_eq!(temporal_positions.len(), f0.len());
    refined_f0.clear();
    let fft_size = 2048;

    for i in 0..f0.len() {
        let current_f0 = f0[i];
        if current_f0 <= 40.0 {
            refined_f0.push(0.0);
        } else {
            refined_f0.push(get_instantaneous_frequency_with_workspace(
                x,
                fs,
                temporal_positions[i],
                current_f0,
                fft_size,
                real_w,
                imag_w,
                real_dw,
                imag_dw,
            ));
        }
    }
}

fn low_pass_filter_into(
    x: &[f32],
    fs: i32,
    cutoff: f32,
    fft_size: usize,
    real: &mut [f32],
    imag: &mut [f32],
) {
    debug_assert_eq!(real.len(), fft_size);
    debug_assert_eq!(imag.len(), fft_size);
    real.fill(0.0);
    imag.fill(0.0);
    for i in 0..x.len().min(fft_size) {
        real[i] = x[i];
    }

    fft(real, imag);

    for i in 0..fft_size / 2 + 1 {
        let freq = i as f32 * fs as f32 / fft_size as f32;
        if freq > cutoff {
            real[i] = 0.0;
            imag[i] = 0.0;
        }
    }
    for i in 1..fft_size / 2 {
        real[fft_size - i] = real[i];
        imag[fft_size - i] = -imag[i];
    }

    ifft(real, imag);
}

fn band_pass_filter_into(
    x: &[f32],
    fs: i32,
    f_low: f32,
    f_high: f32,
    fft_size: usize,
    real: &mut [f32],
    imag: &mut [f32],
) {
    debug_assert_eq!(real.len(), fft_size);
    debug_assert_eq!(imag.len(), fft_size);
    real.fill(0.0);
    imag.fill(0.0);
    for i in 0..x.len().min(fft_size) {
        real[i] = x[i];
    }

    fft(real, imag);

    for i in 0..fft_size / 2 + 1 {
        let freq = i as f32 * fs as f32 / fft_size as f32;
        if freq < f_low || freq > f_high {
            real[i] = 0.0;
            imag[i] = 0.0;
        }
    }
    for i in 1..fft_size / 2 {
        real[fft_size - i] = real[i];
        imag[fft_size - i] = -imag[i];
    }

    ifft(real, imag);
}

fn get_hnr_score(x: &[f32], fs: i32, pos: f32, f0: f32) -> f32 {
    if f0 <= 40.0 {
        return 0.0;
    }
    let center = (pos * fs as f32).round() as isize;
    let window_size = (fs as f32 / f0 * 4.0).round() as usize;
    let start = (center - window_size as isize / 2).max(0) as usize;
    let end = (center + window_size as isize / 2).min(x.len() as isize) as usize;
    let window = &x[start..end];
    if window.len() < 8 {
        return 0.0;
    }

    let mut energy = 0.0;
    for &s in window {
        energy += s * s;
    }
    if energy <= SAFE_GUARD_MINIMUM {
        return 0.0;
    }

    let lag = (fs as f32 / f0).round() as usize;
    if lag >= window.len() {
        return 0.0;
    }

    let mut corr = 0.0;
    for i in 0..window.len() - lag {
        corr += window[i] * window[i + lag];
    }
    (corr / energy).clamp(0.01, 0.99)
}

fn get_f0_candidates_into(
    x: &[f32],
    x_lp: &[f32],
    fs: i32,
    floor: f32,
    ceil: f32,
    temporal_positions: &[f32],
    candidates: &mut Vec<Vec<(f32, f32)>>,
) {
    for (i, &pos) in temporal_positions.iter().enumerate() {
        let center = (pos * fs as f32).round() as isize;
        let window_size = (fs as f32 / floor).round() as usize;
        let start = (center - window_size as isize / 2).max(0) as usize;
        let end = (center + window_size as isize / 2).min(x_lp.len() as isize) as usize;
        let window = &x_lp[start..end];

        if let Some(f0) = zero_crossing_f0(window, fs, floor, ceil) {
            let score = get_hnr_score(x, fs, pos, f0);
            candidates[i].push((f0, score));
        }
    }
}

fn get_harvest_candidates_into(
    x: &[f32],
    x_bp: &[f32],
    fs: i32,
    floor: f32,
    ceil: f32,
    temporal_positions: &[f32],
    candidates: &mut Vec<Vec<(f32, f32)>>,
) {
    let fft_size = 1024;
    let mut real_w = vec![0.0; fft_size];
    let mut imag_w = vec![0.0; fft_size];
    let mut real_dw = vec![0.0; fft_size];
    let mut imag_dw = vec![0.0; fft_size];
    for (i, &pos) in temporal_positions.iter().enumerate() {
        let center = (pos * fs as f32).round() as isize;
        let window_size = (fs as f32 / floor).round() as usize;
        let start = (center - window_size as isize / 2).max(0) as usize;
        let end = (center + window_size as isize / 2).min(x_bp.len() as isize) as usize;
        let window = &x_bp[start..end];

        if let Some(f0_raw) = zero_crossing_f0(window, fs, floor, ceil) {
            let refined = get_instantaneous_frequency_with_workspace(
                x_bp,
                fs,
                pos,
                f0_raw,
                fft_size,
                &mut real_w,
                &mut imag_w,
                &mut real_dw,
                &mut imag_dw,
            );
            if refined >= floor && refined <= ceil {
                let score = get_hnr_score(x, fs, pos, refined);
                candidates[i].push((refined, score));
            }
        }
    }
}

fn fix_contour_into(candidates: &[Vec<(f32, f32)>], f0: &mut [f32]) {
    let frames = candidates.len();
    if frames == 0 {
        return;
    }

    // Viterbi-like global optimization
    // cost[frame][candidate_index] = min total cost to reach this candidate
    let mut dp_cost = vec![Vec::new(); frames];
    let mut dp_prev = vec![Vec::new(); frames];

    // Initial frame
    if !candidates[0].is_empty() {
        dp_cost[0].resize(candidates[0].len(), 0.0);
        for j in 0..candidates[0].len() {
            dp_cost[0][j] = 1.0 - candidates[0][j].1; // Initial cost is 1 - score
        }
    } else {
        dp_cost[0].push(1.0); // Penalty for unvoiced
    }

    for i in 1..frames {
        let n_curr = candidates[i].len().max(1);
        dp_cost[i].resize(n_curr, f32::MAX);
        dp_prev[i].resize(n_curr, 0);

        let n_prev = if candidates[i - 1].is_empty() {
            1
        } else {
            candidates[i - 1].len()
        };

        for j in 0..n_curr {
            let (curr_f0, curr_score) = if candidates[i].is_empty() {
                (0.0, 0.0)
            } else {
                candidates[i][j]
            };

            for k in 0..n_prev {
                let prev_f0 = if candidates[i - 1].is_empty() {
                    0.0
                } else {
                    candidates[i - 1][k].0
                };
                let prev_cost = dp_cost[i - 1][k];

                // Transition cost: penalty for jumps, especially non-octave jumps
                let transition_cost = if curr_f0 > 0.0 && prev_f0 > 0.0 {
                    let ratio = (curr_f0 / prev_f0).ln().abs();
                    // Octave-aware jump penalty
                    let octave_diff = (ratio / std::f32::consts::LN_2)
                        .fract()
                        .min(1.0 - (ratio / std::f32::consts::LN_2).fract());
                    ratio * 2.0 + octave_diff * 0.5
                } else if curr_f0 == 0.0 && prev_f0 == 0.0 {
                    0.0
                } else {
                    0.5 // Penalty for VUV change
                };

                let total_cost = prev_cost + transition_cost + (1.0 - curr_score);
                if total_cost < dp_cost[i][j] {
                    dp_cost[i][j] = total_cost;
                    dp_prev[i][j] = k;
                }
            }
        }
    }

    // Backtrack
    let mut current_best_index = 0;
    let mut min_final_cost = f32::MAX;
    for j in 0..dp_cost[frames - 1].len() {
        if dp_cost[frames - 1][j] < min_final_cost {
            min_final_cost = dp_cost[frames - 1][j];
            current_best_index = j;
        }
    }

    for i in (0..frames).rev() {
        if candidates[i].is_empty() {
            f0[i] = 0.0;
            // For unvoiced, we just use the previous best index as the state was collapsed to 1
            if i > 0 {
                current_best_index = dp_prev[i][0];
            }
        } else {
            f0[i] = candidates[i][current_best_index].0;
            if i > 0 {
                current_best_index = dp_prev[i][current_best_index];
            }
        }
    }
}

fn correct_dio_subharmonics_and_edges(f0: &mut [f32], config: &F0EstimationConfig) {
    if f0.is_empty() {
        return;
    }

    f0[0] = 0.0;
    let original = f0.to_vec();
    for index in 1..f0.len().saturating_sub(1) {
        let current = original[index];
        if current <= 0.0 {
            continue;
        }

        let start = index.saturating_sub(2);
        let end = (index + 3).min(original.len());
        let mut neighbors: Vec<f32> = original[start..end]
            .iter()
            .enumerate()
            .filter_map(|(offset, &value)| {
                let absolute = start + offset;
                if absolute != index && value > 0.0 {
                    Some(value)
                } else {
                    None
                }
            })
            .collect();
        if neighbors.len() < 2 {
            continue;
        }
        neighbors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = neighbors[neighbors.len() / 2];
        let doubled = current * 2.0;
        if doubled <= config.f0_ceil
            && current < median * 0.7
            && (doubled - median).abs() / median.max(SAFE_GUARD_MINIMUM) < 0.2
        {
            f0[index] = doubled;
        }
    }
}

fn smooth_isolated_f0_outliers(f0: &mut [f32]) {
    if f0.len() < 3 {
        return;
    }
    let original = f0.to_vec();
    for index in 1..original.len() - 1 {
        let prev = original[index - 1];
        let curr = original[index];
        let next = original[index + 1];
        if prev <= 0.0 || curr <= 0.0 || next <= 0.0 {
            continue;
        }
        let neighbor_mean = 0.5 * (prev + next);
        let denominator = neighbor_mean.max(SAFE_GUARD_MINIMUM);
        let neighbors_are_stable = (prev - next).abs() / denominator < 0.05;
        let current_is_outlier = (curr - neighbor_mean).abs() / denominator > 0.10;
        if neighbors_are_stable && current_is_outlier {
            f0[index] = neighbor_mean;
        }
    }
}

pub fn analyze(x: &[f32], fs: i32, config: &mut AcousticConfig) -> AcousticFeatures {
    let mut features = AcousticFeatures::new(config.f0_estimation.frame_period, fs);
    let mut workspace = AnalyzerWorkspace::new();
    analyze_into(x, fs, config, &mut features, &mut workspace);
    features
}

pub fn analyze_into(
    x: &[f32],
    fs: i32,
    config: &mut AcousticConfig,
    features: &mut AcousticFeatures,
    workspace: &mut AnalyzerWorkspace,
) {
    features.frame_period = config.f0_estimation.frame_period;
    features.fs = fs;
    dio_into(
        x,
        fs,
        &config.f0_estimation,
        &mut features.temporal_positions,
        &mut features.f0,
    );
    if config.refine_f0 {
        const STONEMASK_FFT_SIZE: usize = 2048;
        workspace.refined_f0.clear();
        workspace.refined_f0.extend_from_slice(&features.f0);
        workspace.stonemask_real_w.resize(STONEMASK_FFT_SIZE, 0.0);
        workspace.stonemask_imag_w.resize(STONEMASK_FFT_SIZE, 0.0);
        workspace.stonemask_real_dw.resize(STONEMASK_FFT_SIZE, 0.0);
        workspace.stonemask_imag_dw.resize(STONEMASK_FFT_SIZE, 0.0);
        stonemask_into_with_workspace(
            x,
            fs,
            &features.temporal_positions,
            &workspace.refined_f0,
            &mut features.f0,
            &mut workspace.stonemask_real_w,
            &mut workspace.stonemask_imag_w,
            &mut workspace.stonemask_real_dw,
            &mut workspace.stonemask_imag_dw,
        );
        smooth_isolated_f0_outliers(&mut features.f0);
    }
    cheaptrick_into(
        x,
        fs,
        &features.temporal_positions,
        &features.f0,
        &mut config.spectral_analysis,
        &mut features.spectrogram,
    );
    features.fft_size = config.spectral_analysis.fft_size;
    d4c_into(
        x,
        fs,
        &features.temporal_positions,
        &features.f0,
        features.fft_size,
        &config.aperiodicity_analysis,
        &mut features.aperiodicity,
    );
}

fn get_frame_count(fs: i32, x_length: usize, frame_period: f32) -> usize {
    if fs <= 0 || x_length == 0 || frame_period <= 0.0 {
        return 0;
    }
    ((1000.0 * x_length as f32 / fs as f32 / frame_period).floor() as usize) + 1
}

fn zero_crossing_f0(window: &[f32], fs: i32, floor: f32, ceil: f32) -> Option<f32> {
    if window.len() < 2 {
        return None;
    }

    let mut estimates = Vec::with_capacity(4);
    if let Some(estimate) =
        zero_crossing_interval_f0(window, fs as f32, CrossingDirection::PositiveToNegative)
    {
        estimates.push(estimate);
    }
    if let Some(estimate) =
        zero_crossing_interval_f0(window, fs as f32, CrossingDirection::NegativeToPositive)
    {
        estimates.push(estimate);
    }

    let derivative: Vec<f32> = window.windows(2).map(|pair| pair[0] - pair[1]).collect();
    if let Some(estimate) = zero_crossing_interval_f0(
        &derivative,
        fs as f32,
        CrossingDirection::PositiveToNegative,
    ) {
        estimates.push(estimate);
    }
    if let Some(estimate) = zero_crossing_interval_f0(
        &derivative,
        fs as f32,
        CrossingDirection::NegativeToPositive,
    ) {
        estimates.push(estimate);
    }

    let valid: Vec<f32> = estimates
        .into_iter()
        .filter(|&estimate| estimate >= floor && estimate <= ceil)
        .collect();
    if valid.is_empty() {
        return None;
    }

    let mean = valid.iter().sum::<f32>() / valid.len() as f32;
    if mean >= floor && mean <= ceil {
        Some(mean)
    } else {
        None
    }
}

#[derive(Clone, Copy)]
enum CrossingDirection {
    PositiveToNegative,
    NegativeToPositive,
}

fn zero_crossing_interval_f0(signal: &[f32], fs: f32, direction: CrossingDirection) -> Option<f32> {
    if signal.len() < 2 {
        return None;
    }

    let mut edges = Vec::new();
    for i in 0..signal.len() - 1 {
        let crosses = match direction {
            CrossingDirection::PositiveToNegative => signal[i] > 0.0 && signal[i + 1] <= 0.0,
            CrossingDirection::NegativeToPositive => signal[i] < 0.0 && signal[i + 1] >= 0.0,
        };
        if crosses {
            let denominator = signal[i + 1] - signal[i];
            if denominator.abs() > SAFE_GUARD_MINIMUM {
                edges.push(i as f32 - signal[i] / denominator);
            }
        }
    }

    if edges.len() < 2 {
        return None;
    }

    let mut sum = 0.0;
    let mut count = 0usize;
    for pair in edges.windows(2) {
        let interval = pair[1] - pair[0];
        if interval > SAFE_GUARD_MINIMUM {
            sum += fs / interval;
            count += 1;
        }
    }

    if count == 0 {
        None
    } else {
        Some(sum / count as f32)
    }
}
