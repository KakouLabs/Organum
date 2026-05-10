use std::f32::consts::PI;

use super::analysis::get_f0_floor_for_cheaptrick;
use super::fft::{fft, ifft};
use super::random::RandnState;
use super::{
    get_fft_size_for_cheaptrick, MatrixF32, MatrixViewF32, SpectralAnalysisConfig, DEFAULT_F0,
    EPSILON_FOR_CHEAPTRICK, SAFE_GUARD_MINIMUM,
};

pub fn cheaptrick(
    x: &[f32],
    fs: i32,
    temporal_positions: &[f32],
    f0: &[f32],
    config: &mut SpectralAnalysisConfig,
) -> MatrixF32 {
    let mut spectrogram = MatrixF32::zeros(0, 0);
    cheaptrick_into(x, fs, temporal_positions, f0, config, &mut spectrogram);
    spectrogram
}

pub fn cheaptrick_into(
    x: &[f32],
    fs: i32,
    temporal_positions: &[f32],
    f0: &[f32],
    config: &mut SpectralAnalysisConfig,
    spectrogram: &mut MatrixF32,
) {
    assert_eq!(temporal_positions.len(), f0.len());
    config.fft_size = get_fft_size_for_cheaptrick(fs, config);
    let fft_size = config.fft_size as usize;
    let f0_floor = get_f0_floor_for_cheaptrick(fs, fft_size);
    let cols = fft_size / 2 + 1;
    spectrogram.resize(f0.len(), cols);

    let mut waveform = vec![0.0; fft_size];
    let mut window = vec![0.0; fft_size];
    let mut fft_real = vec![0.0; fft_size];
    let mut fft_imag = vec![0.0; fft_size];
    let mut power_spectrum = vec![0.0; cols];
    let mut dc_input = Vec::new();
    let mut smoothed_spectrum = vec![0.0; cols];
    let mut mirroring_spectrum = Vec::new();
    let mut mirroring_segment = Vec::new();
    let mut log_spectrum = vec![0.0; fft_size];
    let mut recovery_imag = vec![0.0; fft_size];
    let mut randn_state = RandnState::new();

    for frame in 0..f0.len() {
        let current_f0 = if f0[frame] <= f0_floor {
            DEFAULT_F0
        } else {
            f0[frame]
        };

        get_windowed_waveform(
            x,
            fs,
            temporal_positions[frame],
            current_f0,
            fft_size,
            &mut waveform,
            &mut window,
            &mut randn_state,
        );
        get_power_spectrum(
            &waveform,
            fs,
            current_f0,
            fft_size,
            &mut power_spectrum,
            &mut fft_real,
            &mut fft_imag,
            &mut dc_input,
        );
        linear_smoothing(
            &power_spectrum,
            current_f0 * 2.0 / 3.0,
            fs,
            fft_size,
            &mut smoothed_spectrum,
            &mut mirroring_spectrum,
            &mut mirroring_segment,
        );
        add_infinitesimal_noise(&mut smoothed_spectrum, &mut randn_state);
        smoothing_with_recovery(
            &smoothed_spectrum,
            current_f0,
            config.q1,
            fs,
            fft_size,
            spectrogram.row_mut(frame),
            &mut log_spectrum,
            &mut recovery_imag,
        );
    }
}

pub fn cheaptrick_from_spectrum(
    power_spectrum: &[f32],
    _fs: i32,
    f0: &[f32],
    config: &SpectralAnalysisConfig,
) -> MatrixF32 {
    let cols = config.fft_size as usize / 2 + 1;
    assert_eq!(power_spectrum.len(), f0.len().saturating_mul(cols));
    MatrixF32::from_vec(f0.len(), cols, power_spectrum.to_vec())
}

pub fn cheaptrick_from_spectrum_borrowed<'a>(
    power_spectrum: &'a [f32],
    f0_length: usize,
    config: &SpectralAnalysisConfig,
) -> MatrixViewF32<'a> {
    let cols = config.fft_size as usize / 2 + 1;
    MatrixViewF32::new(power_spectrum, f0_length, cols)
}

fn get_windowed_waveform(
    x: &[f32],
    fs: i32,
    temporal_position: f32,
    f0: f32,
    _fft_size: usize,
    waveform: &mut [f32],
    window: &mut [f32],
    randn_state: &mut RandnState,
) {
    let half_window_length = (1.5 * fs as f32 / f0).round() as isize;
    let origin = (temporal_position * fs as f32 + 0.001).round() as isize;
    waveform.fill(0.0);

    let window_length = half_window_length.saturating_mul(2).saturating_add(1) as usize;
    debug_assert!(window_length <= window.len());
    let window = &mut window[..window_length];
    let mut power = 0.0;
    for i in -half_window_length..=half_window_length {
        let position = i as f32 / 1.5 / fs as f32;
        let value = 0.5 * (PI * position * f0).cos() + 0.5;
        let index = (i + half_window_length) as usize;
        window[index] = value;
        power += value * value;
    }
    let norm = power.sqrt().max(SAFE_GUARD_MINIMUM);
    for value in window.iter_mut() {
        *value /= norm;
    }

    let mut weighted_sum = 0.0;
    let mut window_sum = 0.0;
    for (i, &window_value) in window.iter().enumerate() {
        let base_index = i as isize - half_window_length;
        let safe_index = (origin + base_index).clamp(0, x.len().saturating_sub(1) as isize);
        waveform[i] =
            x[safe_index as usize] * window_value + randn_state.randn() * SAFE_GUARD_MINIMUM;
        weighted_sum += waveform[i];
        window_sum += window_value;
    }
    let weighting_coefficient = weighted_sum / window_sum.max(SAFE_GUARD_MINIMUM);
    for (sample, &window_value) in waveform.iter_mut().zip(window.iter()) {
        *sample -= window_value * weighting_coefficient;
    }
}

fn add_infinitesimal_noise(spectrum: &mut [f32], randn_state: &mut RandnState) {
    for value in spectrum {
        *value += randn_state.randn().abs() * EPSILON_FOR_CHEAPTRICK;
    }
}

fn get_power_spectrum(
    waveform: &[f32],
    fs: i32,
    f0: f32,
    fft_size: usize,
    power_spectrum: &mut [f32],
    real: &mut [f32],
    imag: &mut [f32],
    dc_input: &mut Vec<f32>,
) {
    debug_assert_eq!(real.len(), fft_size);
    debug_assert_eq!(imag.len(), fft_size);
    real.copy_from_slice(waveform);
    imag.fill(0.0);
    fft(real, imag);
    for i in 0..fft_size / 2 + 1 {
        power_spectrum[i] = real[i] * real[i] + imag[i] * imag[i];
    }
    dc_correction(power_spectrum, f0, fs, fft_size, dc_input);
}

fn dc_correction(
    power_spectrum: &mut [f32],
    f0: f32,
    fs: i32,
    fft_size: usize,
    input: &mut Vec<f32>,
) {
    let upper_limit = 2 + (f0 * fft_size as f32 / fs as f32) as usize;
    if upper_limit + 1 >= power_spectrum.len() {
        return;
    }
    let upper_limit_replica = upper_limit - 1;
    let frequency_interval = fs as f32 / fft_size as f32;
    input.clear();
    input.extend_from_slice(&power_spectrum[..=upper_limit]);
    for i in 0..upper_limit_replica {
        let target_frequency = i as f32 * frequency_interval;
        let source_index = (f0 - target_frequency) / frequency_interval;
        let low = source_index.floor().max(0.0) as usize;
        let frac = source_index - low as f32;
        let replica = if low + 1 < input.len() {
            input[low] * (1.0 - frac) + input[low + 1] * frac
        } else {
            input[low]
        };
        power_spectrum[i] += replica;
    }
}

fn linear_smoothing(
    input: &[f32],
    width: f32,
    fs: i32,
    fft_size: usize,
    output: &mut [f32],
    mirroring_spectrum: &mut Vec<f32>,
    mirroring_segment: &mut Vec<f32>,
) {
    let half_bins = fft_size / 2;
    let boundary = (width * fft_size as f32 / fs as f32) as usize + 1;
    let mirrored_len = half_bins + boundary * 2 + 1;
    mirroring_spectrum.resize(mirrored_len, 0.0);
    mirroring_segment.resize(mirrored_len, 0.0);
    let mirroring_spectrum = &mut mirroring_spectrum[..mirrored_len];
    let mirroring_segment = &mut mirroring_segment[..mirrored_len];

    for i in 0..boundary {
        mirroring_spectrum[i] = input[boundary - i];
    }
    for i in boundary..half_bins + boundary {
        mirroring_spectrum[i] = input[i - boundary];
    }
    for i in half_bins + boundary..=half_bins + boundary * 2 {
        mirroring_spectrum[i] = input[half_bins - (i - (half_bins + boundary))];
    }

    let frequency_interval = fs as f32 / fft_size as f32;
    mirroring_segment[0] = mirroring_spectrum[0] * frequency_interval;
    for i in 1..mirrored_len {
        mirroring_segment[i] =
            mirroring_spectrum[i] * frequency_interval + mirroring_segment[i - 1];
    }

    let origin = -(boundary as f32 - 0.5) * frequency_interval;
    for (i, value) in output.iter_mut().enumerate().take(half_bins + 1) {
        let frequency = i as f32 / fft_size as f32 * fs as f32 - width / 2.0;
        let low = interp1q(origin, frequency_interval, &mirroring_segment, frequency);
        let high = interp1q(
            origin,
            frequency_interval,
            &mirroring_segment,
            frequency + width,
        );
        *value = (high - low) / width;
    }
}

fn interp1q(origin: f32, interval: f32, values: &[f32], query: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let position = (query - origin) / interval;
    if position <= 0.0 {
        return values[0];
    }
    let low = position.floor() as usize;
    if low + 1 >= values.len() {
        return values[values.len() - 1];
    }
    let frac = position - low as f32;
    values[low] * (1.0 - frac) + values[low + 1] * frac
}

fn smoothing_with_recovery(
    input_spectrum: &[f32],
    f0: f32,
    q1: f32,
    fs: i32,
    fft_size: usize,
    output_spectrum: &mut [f32],
    log_spectrum: &mut [f32],
    imag: &mut [f32],
) {
    debug_assert_eq!(log_spectrum.len(), fft_size);
    debug_assert_eq!(imag.len(), fft_size);
    let log_spectrum = &mut log_spectrum[..fft_size];
    let imag = &mut imag[..fft_size];
    log_spectrum.fill(0.0);
    for i in 0..fft_size / 2 + 1 {
        log_spectrum[i] = input_spectrum[i].max(SAFE_GUARD_MINIMUM).ln();
    }
    for i in 1..fft_size / 2 {
        log_spectrum[fft_size - i] = log_spectrum[i];
    }

    imag.fill(0.0);
    fft(log_spectrum, imag);

    for i in 0..=fft_size / 2 {
        let quefrency = i as f32 / fs as f32;
        let smoothing_lifter = if i == 0 {
            1.0
        } else {
            let x = PI * f0 * quefrency;
            x.sin() / x
        };
        let compensation_lifter = (1.0 - 2.0 * q1) + 2.0 * q1 * (2.0 * PI * quefrency * f0).cos();
        let factor = smoothing_lifter * compensation_lifter;
        log_spectrum[i] *= factor;
        imag[i] = 0.0;
        if i > 0 && i < fft_size / 2 {
            let mirror = fft_size - i;
            log_spectrum[mirror] *= factor;
            imag[mirror] = 0.0;
        }
    }

    ifft(log_spectrum, imag);

    for i in 0..fft_size / 2 + 1 {
        output_spectrum[i] = log_spectrum[i].exp();
    }
}
