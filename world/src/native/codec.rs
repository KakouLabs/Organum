use std::f32::consts::PI;

use super::constants::*;
use super::fft::{fft, ifft};
use super::{get_number_of_aperiodicities, MatrixF32};

pub(super) fn frequency_to_mel(frequency: f32) -> f32 {
    MEL_M0 * (frequency / MEL_F0 + 1.0).ln()
}

pub(super) fn mel_to_frequency(mel: f32) -> f32 {
    MEL_F0 * ((mel / MEL_M0).exp() - 1.0)
}

pub(super) fn mel_axis_for_codec(fs: i32, bins: usize) -> Vec<f32> {
    let max_dimension = bins.saturating_sub(1);
    if max_dimension == 0 {
        return Vec::new();
    }

    let floor_mel = frequency_to_mel(FLOOR_FREQUENCY);
    let ceil_mel = frequency_to_mel((fs as f32 / 2.0).min(CEIL_FREQUENCY));
    (0..max_dimension)
        .map(|index| (ceil_mel - floor_mel) * index as f32 / max_dimension as f32 + floor_mel)
        .collect()
}

pub(super) fn mel_axis_for_decode(fs: i32, bins: usize) -> Vec<f32> {
    mel_axis_for_codec(fs, bins)
        .into_iter()
        .map(mel_to_frequency)
        .collect()
}

#[cfg(test)]
pub(super) fn dct_for_codec(mel_spectrum: &[f32], output: &mut [f32]) {
    let max_dimension = mel_spectrum.len();
    if max_dimension == 0 {
        return;
    }

    let mut real = vec![0.0; max_dimension];
    let mut imag = vec![0.0; max_dimension];
    let weights = dct_weights_for_codec(max_dimension, output.len());
    dct_for_codec_with_workspace(mel_spectrum, output, &weights, &mut real, &mut imag);
}

pub(super) fn dct_weights_for_codec(max_dimension: usize, output_len: usize) -> Vec<(f32, f32)> {
    let fft_size = max_dimension * 2;
    let normalization = (max_dimension as f32).sqrt();
    let weight_scale = 2.0 / (fft_size as f32).sqrt();
    (0..output_len)
        .map(|i| {
            let angle = i as f32 * PI / fft_size as f32;
            let mut weight_real = weight_scale * angle.cos();
            let weight_imag = weight_scale * angle.sin();
            if i == 0 {
                weight_real *= std::f32::consts::FRAC_1_SQRT_2;
            }
            (weight_real / normalization, weight_imag / normalization)
        })
        .collect()
}

pub(super) fn dct_for_codec_with_workspace(
    mel_spectrum: &[f32],
    output: &mut [f32],
    weights: &[(f32, f32)],
    real: &mut [f32],
    imag: &mut [f32],
) {
    let max_dimension = mel_spectrum.len();
    if max_dimension == 0 {
        return;
    }

    debug_assert_eq!(real.len(), max_dimension);
    debug_assert_eq!(imag.len(), max_dimension);
    debug_assert_eq!(weights.len(), output.len());

    real.fill(0.0);
    imag.fill(0.0);

    let bias = max_dimension / 2;
    for i in 0..bias {
        real[i] = mel_spectrum[i * 2];
        real[i + bias] = mel_spectrum[max_dimension - (i * 2) - 1];
    }
    fft(real, imag);

    for ((value, &(weight_real, weight_imag)), (&real_value, &imag_value)) in output
        .iter_mut()
        .zip(weights)
        .zip(real.iter().zip(imag.iter()))
    {
        *value = real_value * weight_real - imag_value * weight_imag;
    }
}

pub(super) fn idct_weights_for_codec(max_dimension: usize) -> Vec<(f32, f32)> {
    let fft_size = max_dimension * 2;
    let normalization = (max_dimension as f32).sqrt();
    let weight_scale = (fft_size as f32).sqrt() * normalization;
    (0..max_dimension)
        .map(|i| {
            let angle = i as f32 * PI / fft_size as f32;
            let mut weight_real = weight_scale * angle.cos();
            let mut weight_imag = weight_scale * angle.sin();
            if i == 0 {
                weight_real *= std::f32::consts::FRAC_1_SQRT_2;
                weight_imag *= std::f32::consts::FRAC_1_SQRT_2;
            }
            (weight_real, weight_imag)
        })
        .collect()
}

pub(super) fn idct_for_codec_with_workspace(
    mel_cepstrum: &[f32],
    output: &mut [f32],
    weights: &[(f32, f32)],
    real: &mut [f32],
    imag: &mut [f32],
) {
    let max_dimension = output.len();
    if max_dimension == 0 {
        return;
    }

    debug_assert_eq!(real.len(), max_dimension);
    debug_assert_eq!(imag.len(), max_dimension);
    debug_assert_eq!(weights.len(), max_dimension);

    for (i, &coefficient) in mel_cepstrum.iter().enumerate().take(max_dimension) {
        let (weight_real, weight_imag) = weights[i];
        real[i] = coefficient * weight_real;
        imag[i] = -coefficient * weight_imag;
    }
    for i in mel_cepstrum.len()..max_dimension {
        real[i] = 0.0;
        imag[i] = 0.0;
    }

    ifft(real, imag);
    for i in 0..(max_dimension / 2) {
        output[i * 2] = real[i] * max_dimension as f32;
        output[i * 2 + 1] = real[max_dimension - i - 1] * max_dimension as f32;
    }
}

#[cfg(test)]
pub(super) fn idct_for_codec(mel_cepstrum: &[f32], output: &mut [f32]) {
    let max_dimension = output.len();
    if max_dimension == 0 {
        return;
    }

    let mut real = vec![0.0; max_dimension];
    let mut imag = vec![0.0; max_dimension];
    let weights = idct_weights_for_codec(max_dimension);
    idct_for_codec_with_workspace(mel_cepstrum, output, &weights, &mut real, &mut imag);
}

pub(super) fn interpolate_axis(axis: &[f32], values: &[f32], target: f32) -> f32 {
    assert_eq!(axis.len(), values.len());
    if axis.is_empty() {
        return 0.0;
    }
    if axis.len() == 1 || target <= axis[0] {
        return values[0];
    }
    if target >= axis[axis.len() - 1] {
        return values[values.len() - 1];
    }

    let mut right = 1;
    while right < axis.len() && axis[right] < target {
        right += 1;
    }
    let left = right - 1;
    let span = (axis[right] - axis[left]).max(SAFE_GUARD_MINIMUM);
    let fraction = (target - axis[left]) / span;
    values[left] * (1.0 - fraction) + values[right] * fraction
}

#[derive(Clone, Copy)]
pub(super) struct InterpolationIndex {
    left: usize,
    right: usize,
    fraction: f32,
}

impl InterpolationIndex {
    #[inline]
    pub(super) fn interpolate(self, values: &[f32]) -> f32 {
        values[self.left] * (1.0 - self.fraction) + values[self.right] * self.fraction
    }
}

pub(super) fn interpolation_indices(axis: &[f32], targets: &[f32]) -> Vec<InterpolationIndex> {
    assert!(!axis.is_empty());
    if axis.len() == 1 {
        return vec![
            InterpolationIndex {
                left: 0,
                right: 0,
                fraction: 0.0,
            };
            targets.len()
        ];
    }

    let targets_are_sorted = targets.windows(2).all(|pair| pair[0] <= pair[1]);
    let mut output = Vec::with_capacity(targets.len());

    if targets_are_sorted {
        let last = axis.len() - 1;
        let mut right = 1;
        for &target in targets {
            if target <= axis[0] {
                output.push(InterpolationIndex {
                    left: 0,
                    right: 0,
                    fraction: 0.0,
                });
                continue;
            }
            if target >= axis[last] {
                output.push(InterpolationIndex {
                    left: last,
                    right: last,
                    fraction: 0.0,
                });
                continue;
            }
            while right < last && axis[right] < target {
                right += 1;
            }
            let left = right - 1;
            let span = (axis[right] - axis[left]).max(SAFE_GUARD_MINIMUM);
            output.push(InterpolationIndex {
                left,
                right,
                fraction: (target - axis[left]) / span,
            });
        }
        return output;
    }

    let last = axis.len() - 1;
    for &target in targets {
        if target <= axis[0] {
            output.push(InterpolationIndex {
                left: 0,
                right: 0,
                fraction: 0.0,
            });
            continue;
        }
        if target >= axis[last] {
            output.push(InterpolationIndex {
                left: last,
                right: last,
                fraction: 0.0,
            });
            continue;
        }

        let right = axis.partition_point(|&value| value < target);
        let left = right - 1;
        let span = (axis[right] - axis[left]).max(SAFE_GUARD_MINIMUM);
        output.push(InterpolationIndex {
            left,
            right,
            fraction: (target - axis[left]) / span,
        });
    }
    output
}

pub(super) fn interpolate(values: &[f32], position: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let left = position.floor() as usize;
    let right = (left + 1).min(values.len() - 1);
    let fraction = position - left as f32;
    values[left] * (1.0 - fraction) + values[right] * fraction
}

pub fn code_aperiodicity(aperiodicity: &MatrixF32, fs: i32) -> MatrixF32 {
    let mut output = MatrixF32::zeros(0, 0);
    code_aperiodicity_into(aperiodicity, fs, &mut output);
    output
}

pub fn code_aperiodicity_into(aperiodicity: &MatrixF32, fs: i32, output: &mut MatrixF32) {
    let cols = get_number_of_aperiodicities(fs) as usize;
    output.resize(aperiodicity.rows(), cols);
    if aperiodicity.cols() == 0 || cols == 0 {
        return;
    }

    let bin_width = fs as f32 / ((aperiodicity.cols() - 1) * 2).max(1) as f32;
    for row in 0..aperiodicity.rows() {
        let log_aperiodicity: Vec<f32> = aperiodicity
            .row(row)
            .iter()
            .map(|value| 20.0 * value.max(SAFE_GUARD_MINIMUM).log10())
            .collect();
        for col in 0..cols {
            let frequency = FREQUENCY_INTERVAL * (col + 1) as f32;
            let position = frequency / bin_width;
            output.row_mut(row)[col] = interpolate(&log_aperiodicity, position);
        }
    }
}

pub fn decode_aperiodicity(coded_aperiodicity: &MatrixF32, fs: i32, fft_size: i32) -> MatrixF32 {
    let mut output = MatrixF32::zeros(0, 0);
    decode_aperiodicity_into(coded_aperiodicity, fs, fft_size, &mut output);
    output
}

pub fn decode_aperiodicity_into(
    coded_aperiodicity: &MatrixF32,
    fs: i32,
    fft_size: i32,
    output: &mut MatrixF32,
) {
    let cols = fft_size as usize / 2 + 1;
    output.resize(coded_aperiodicity.rows(), cols);
    output.fill(1.0 - SAFE_GUARD_MINIMUM);
    if coded_aperiodicity.cols() == 0 || cols == 0 {
        return;
    }

    let number_of_aperiodicities = coded_aperiodicity.cols();
    let mut coarse_frequency_axis = Vec::with_capacity(number_of_aperiodicities + 2);
    coarse_frequency_axis.push(0.0);
    for col in 0..number_of_aperiodicities {
        coarse_frequency_axis.push(FREQUENCY_INTERVAL * (col + 1) as f32);
    }
    coarse_frequency_axis.push(fs as f32 / 2.0);

    for row in 0..coded_aperiodicity.rows() {
        let coded = coded_aperiodicity.row(row);
        let average_db = coded.iter().sum::<f32>() / coded.len().max(1) as f32;
        if average_db > -0.5 {
            continue;
        }

        let mut coarse_aperiodicity = Vec::with_capacity(number_of_aperiodicities + 2);
        coarse_aperiodicity.push(-60.0);
        coarse_aperiodicity.extend_from_slice(coded);
        coarse_aperiodicity.push(-SAFE_GUARD_MINIMUM);

        for bin in 0..cols {
            let frequency = bin as f32 * fs as f32 / fft_size.max(1) as f32;
            let db = interpolate_axis(&coarse_frequency_axis, &coarse_aperiodicity, frequency);
            output.row_mut(row)[bin] = 10.0_f32.powf(db / 20.0);
        }
    }
}

pub fn code_spectral_envelope(
    spectrogram: &MatrixF32,
    fs: i32,
    number_of_dimensions: i32,
) -> MatrixF32 {
    let mut output = MatrixF32::zeros(0, 0);
    code_spectral_envelope_into(spectrogram, fs, number_of_dimensions, &mut output);
    output
}

pub fn code_spectral_envelope_into(
    spectrogram: &MatrixF32,
    fs: i32,
    number_of_dimensions: i32,
    output: &mut MatrixF32,
) {
    let dims = number_of_dimensions.max(0) as usize;
    output.resize(spectrogram.rows(), dims);
    if spectrogram.cols() < 2 || dims == 0 {
        return;
    }

    let max_dimension = spectrogram.cols() - 1;
    let mel_axis = mel_axis_for_codec(fs, spectrogram.cols());
    let frequency_mel_axis: Vec<f32> = (0..spectrogram.cols())
        .map(|bin| frequency_to_mel(bin as f32 * fs as f32 / ((spectrogram.cols() - 1) * 2) as f32))
        .collect();
    let mel_interpolation = interpolation_indices(&frequency_mel_axis, &mel_axis);
    let mut mel_spectrum = vec![0.0; max_dimension];
    let mut log_spectrum = vec![0.0; spectrogram.cols()];
    let mut dct_real = vec![0.0; max_dimension];
    let mut dct_imag = vec![0.0; max_dimension];
    let dct_weights = dct_weights_for_codec(max_dimension, dims);

    for row in 0..spectrogram.rows() {
        for (dst, &src) in log_spectrum.iter_mut().zip(spectrogram.row(row)) {
            *dst = src.max(SAFE_GUARD_MINIMUM).ln();
        }
        for (value, interpolation) in mel_spectrum.iter_mut().zip(&mel_interpolation) {
            *value = interpolation.interpolate(&log_spectrum);
        }
        dct_for_codec_with_workspace(
            &mel_spectrum,
            output.row_mut(row),
            &dct_weights,
            &mut dct_real,
            &mut dct_imag,
        );
    }
}

pub fn decode_spectral_envelope(
    coded_spectrogram: &MatrixF32,
    fs: i32,
    fft_size: i32,
) -> MatrixF32 {
    let mut output = MatrixF32::zeros(0, 0);
    decode_spectral_envelope_into(coded_spectrogram, fs, fft_size, &mut output);
    output
}

pub fn decode_spectral_envelope_into(
    coded_spectrogram: &MatrixF32,
    fs: i32,
    fft_size: i32,
    output: &mut MatrixF32,
) {
    let cols = fft_size as usize / 2 + 1;
    output.resize(coded_spectrogram.rows(), cols);
    if cols < 2 || coded_spectrogram.cols() == 0 {
        return;
    }

    let max_dimension = cols - 1;
    let mel_axis = mel_axis_for_decode(fs, cols);
    let frequency_axis: Vec<f32> = (0..cols)
        .map(|bin| bin as f32 * fs as f32 / fft_size.max(1) as f32)
        .collect();
    let mut mel_spectrum = vec![0.0; max_dimension];
    let mut dct_real = vec![0.0; max_dimension];
    let mut dct_imag = vec![0.0; max_dimension];
    let idct_weights = idct_weights_for_codec(max_dimension);
    let mut padded_mel_axis = Vec::with_capacity(max_dimension + 2);
    padded_mel_axis.push(0.0);
    padded_mel_axis.extend_from_slice(&mel_axis);
    padded_mel_axis.push(fs as f32 / 2.0);
    let frequency_interpolation = interpolation_indices(&padded_mel_axis, &frequency_axis);
    let mut padded_mel_spectrum = vec![0.0; max_dimension + 2];

    for row in 0..coded_spectrogram.rows() {
        idct_for_codec_with_workspace(
            coded_spectrogram.row(row),
            &mut mel_spectrum,
            &idct_weights,
            &mut dct_real,
            &mut dct_imag,
        );
        padded_mel_spectrum[0] = mel_spectrum[0];
        padded_mel_spectrum[1..=max_dimension].copy_from_slice(&mel_spectrum);
        padded_mel_spectrum[max_dimension + 1] = mel_spectrum[max_dimension - 1];
        for (bin, interpolation) in frequency_interpolation.iter().enumerate() {
            let log_value = interpolation.interpolate(&padded_mel_spectrum);
            output.row_mut(row)[bin] = (log_value / max_dimension as f32)
                .exp()
                .max(SAFE_GUARD_MINIMUM);
        }
    }
}
