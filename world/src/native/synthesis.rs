use std::f32::consts::PI;

use super::fft::{fft, ifft};
use super::random::RandnState;
use super::{MatrixF32, DEFAULT_F0, SAFE_GUARD_MINIMUM};

pub fn synthesis(
    f0: &[f32],
    spectrogram: &MatrixF32,
    aperiodicity: &MatrixF32,
    frame_period: f32,
    fs: i32,
) -> Vec<f32> {
    assert_eq!(spectrogram.rows(), f0.len());
    assert_eq!(aperiodicity.rows(), f0.len());
    assert_eq!(spectrogram.cols(), aperiodicity.cols());
    let y_length = ((f0.len() as f32 * frame_period * fs as f32) / 1000.0).floor() as usize;
    let mut y = vec![0.0; y_length];
    synthesis_into(f0, spectrogram, aperiodicity, frame_period, fs, &mut y);
    y
}

pub fn synthesis_into(
    f0: &[f32],
    spectrogram: &MatrixF32,
    aperiodicity: &MatrixF32,
    frame_period: f32,
    fs: i32,
    y: &mut Vec<f32>,
) {
    assert_eq!(spectrogram.rows(), f0.len());
    assert_eq!(aperiodicity.rows(), f0.len());
    assert_eq!(spectrogram.cols(), aperiodicity.cols());
    let y_length = ((f0.len() as f32 * frame_period * fs as f32) / 1000.0).floor() as usize;
    y.clear();
    y.resize(y_length, 0.0);
    synthesis_in_place(f0, spectrogram, aperiodicity, frame_period, fs, y);
}

pub fn synthesis_in_place(
    f0: &[f32],
    spectrogram: &MatrixF32,
    aperiodicity: &MatrixF32,
    frame_period: f32,
    fs: i32,
    y: &mut [f32],
) {
    assert_eq!(spectrogram.rows(), f0.len());
    assert_eq!(aperiodicity.rows(), f0.len());
    assert_eq!(spectrogram.cols(), aperiodicity.cols());
    y.fill(0.0);
    if f0.is_empty() || y.is_empty() {
        return;
    }

    let fft_size = (spectrogram.cols() - 1) * 2;
    if fft_size == 0 {
        return;
    }
    if !fft_size.is_power_of_two() {
        return;
    }

    let frame_period_sec = frame_period / 1000.0;
    let lowest_f0 = fs as f32 / fft_size as f32 + 1.0;
    let time_base = synthesis_time_base(f0, fs, frame_period_sec, y.len(), lowest_f0);
    let dc_remover = synthesis_dc_remover(fft_size);
    let mut randn_state = RandnState::new();

    let mut response = vec![0.0; fft_size];
    let mut periodic_response = vec![0.0; fft_size];
    let mut aperiodic_response = vec![0.0; fft_size];
    let mut spectral_envelope = vec![0.0; spectrogram.cols()];
    let mut aperiodic_ratio = vec![0.0; spectrogram.cols()];
    let mut response_workspace = SynthesisResponseWorkspace::new(fft_size, spectrogram.cols());

    for pulse_index in 0..time_base.pulse_locations.len() {
        let current_index = time_base.pulse_indices[pulse_index];
        let next_index = time_base.pulse_indices[pulse_index
            .saturating_add(1)
            .min(time_base.pulse_indices.len() - 1)];
        let noise_size = next_index.saturating_sub(current_index).max(1);

        get_one_frame_segment(
            time_base.interpolated_vuv[current_index],
            noise_size,
            spectrogram,
            aperiodicity,
            frame_period_sec,
            time_base.pulse_locations[pulse_index],
            time_base.pulse_time_shifts[pulse_index],
            fs,
            &dc_remover,
            &mut spectral_envelope,
            &mut aperiodic_ratio,
            &mut response,
            &mut periodic_response,
            &mut aperiodic_response,
            &mut response_workspace,
            &mut randn_state,
        );

        let offset = current_index as isize - fft_size as isize / 2 + 1;
        let lower = 0isize.max(-offset) as usize;
        let upper = fft_size.min((y.len() as isize - offset).max(0) as usize);
        for (j, &value) in response.iter().enumerate().take(upper).skip(lower) {
            let output_index = (j as isize + offset) as usize;
            y[output_index] += value;
        }
    }
}

struct SynthesisTimeBase {
    pulse_locations: Vec<f32>,
    pulse_indices: Vec<usize>,
    pulse_time_shifts: Vec<f32>,
    interpolated_vuv: Vec<f32>,
}

fn synthesis_time_base(
    f0: &[f32],
    fs: i32,
    frame_period_sec: f32,
    y_length: usize,
    lowest_f0: f32,
) -> SynthesisTimeBase {
    let mut coarse_f0 = Vec::with_capacity(f0.len() + 1);
    let mut coarse_vuv = Vec::with_capacity(f0.len() + 1);

    for &value in f0 {
        let voiced_f0 = if value < lowest_f0 { 0.0 } else { value };
        coarse_f0.push(voiced_f0);
        coarse_vuv.push(if voiced_f0 == 0.0 { 0.0 } else { 1.0 });
    }

    let last_f0 = *coarse_f0.last().unwrap_or(&0.0);
    let prev_f0 = if coarse_f0.len() >= 2 {
        coarse_f0[coarse_f0.len() - 2]
    } else {
        last_f0
    };
    coarse_f0.push(last_f0 * 2.0 - prev_f0);
    let last_vuv = *coarse_vuv.last().unwrap_or(&0.0);
    let prev_vuv = if coarse_vuv.len() >= 2 {
        coarse_vuv[coarse_vuv.len() - 2]
    } else {
        last_vuv
    };
    coarse_vuv.push(last_vuv * 2.0 - prev_vuv);

    let mut interpolated_f0 = vec![0.0; y_length];
    let mut interpolated_vuv = vec![0.0; y_length];
    let last = coarse_f0.len() - 1;
    let last_time = f0.len() as f32 * frame_period_sec;
    for i in 0..y_length {
        let target = i as f32 / fs as f32;
        let (left, right, fraction) = if target <= 0.0 {
            (0, 0, 0.0)
        } else if target >= last_time {
            (last, last, 0.0)
        } else {
            let mut right = (target / frame_period_sec.max(SAFE_GUARD_MINIMUM)).ceil() as usize;
            right = right.clamp(1, last);
            while right < last && right as f32 * frame_period_sec < target {
                right += 1;
            }
            while right > 1 && (right - 1) as f32 * frame_period_sec >= target {
                right -= 1;
            }
            let left = right - 1;
            let left_time = left as f32 * frame_period_sec;
            let right_time = right as f32 * frame_period_sec;
            let span = (right_time - left_time).max(SAFE_GUARD_MINIMUM);
            (left, right, (target - left_time) / span)
        };
        let f0_value = coarse_f0[left] * (1.0 - fraction) + coarse_f0[right] * fraction;
        let vuv_value = coarse_vuv[left] * (1.0 - fraction) + coarse_vuv[right] * fraction;
        interpolated_f0[i] = f0_value;
        interpolated_vuv[i] = if vuv_value > 0.5 { 1.0 } else { 0.0 };
        if interpolated_vuv[i] == 0.0 {
            interpolated_f0[i] = DEFAULT_F0;
        }
    }

    let mut pulse_locations = Vec::new();
    let mut pulse_indices = Vec::new();
    let mut pulse_time_shifts = Vec::new();
    if y_length >= 2 {
        let mut previous_phase = (2.0 * PI * interpolated_f0[0] / fs as f32) % (2.0 * PI);
        for i in 1..y_length {
            let current_phase =
                (previous_phase + 2.0 * PI * interpolated_f0[i] / fs as f32) % (2.0 * PI);
            if (current_phase - previous_phase).abs() > PI {
                let y1 = previous_phase - 2.0 * PI;
                let y2 = current_phase;
                let crossing = if (y2 - y1).abs() > SAFE_GUARD_MINIMUM {
                    (-y1 / (y2 - y1)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                pulse_locations.push((i - 1) as f32 / fs as f32);
                pulse_indices.push(i - 1);
                pulse_time_shifts.push(crossing / fs as f32);
            }
            previous_phase = current_phase;
        }
    }

    SynthesisTimeBase {
        pulse_locations,
        pulse_indices,
        pulse_time_shifts,
        interpolated_vuv,
    }
}

fn synthesis_dc_remover(fft_size: usize) -> Vec<f32> {
    let mut remover = vec![0.0; fft_size];
    let mut dc_component = 0.0;
    for i in 0..fft_size / 2 {
        let value = 0.5 - 0.5 * (2.0 * PI * (i as f32 + 1.0) / (1.0 + fft_size as f32)).cos();
        remover[i] = value;
        remover[fft_size - i - 1] = value;
        dc_component += value * 2.0;
    }
    if dc_component > SAFE_GUARD_MINIMUM {
        for value in &mut remover {
            *value /= dc_component;
        }
    }
    remover
}

#[allow(clippy::too_many_arguments)]
fn get_one_frame_segment(
    current_vuv: f32,
    noise_size: usize,
    spectrogram: &MatrixF32,
    aperiodicity: &MatrixF32,
    frame_period_sec: f32,
    current_time: f32,
    fractional_time_shift: f32,
    fs: i32,
    dc_remover: &[f32],
    spectral_envelope: &mut [f32],
    aperiodic_ratio: &mut [f32],
    response: &mut [f32],
    periodic_response: &mut [f32],
    aperiodic_response: &mut [f32],
    response_workspace: &mut SynthesisResponseWorkspace,
    randn_state: &mut RandnState,
) {
    let fft_size = response.len();
    get_spectral_envelope(
        current_time,
        frame_period_sec,
        spectrogram,
        spectral_envelope,
    );
    get_aperiodic_ratio(
        current_time,
        frame_period_sec,
        aperiodicity,
        aperiodic_ratio,
    );

    get_periodic_response_into(
        spectral_envelope,
        aperiodic_ratio,
        current_vuv,
        fractional_time_shift,
        fs,
        dc_remover,
        periodic_response,
        response_workspace,
    );
    get_aperiodic_response_into(
        noise_size,
        fft_size,
        spectral_envelope,
        aperiodic_ratio,
        current_vuv,
        randn_state,
        aperiodic_response,
        response_workspace,
    );
    let sqrt_noise_size = (noise_size as f32).sqrt();
    for i in 0..fft_size {
        response[i] = periodic_response[i] * sqrt_noise_size + aperiodic_response[i];
    }
}

fn get_spectral_envelope(
    current_time: f32,
    frame_period_sec: f32,
    matrix: &MatrixF32,
    out: &mut [f32],
) {
    interpolate_matrix_frame(current_time, frame_period_sec, matrix, out, |value| {
        value.abs()
    });
}

fn get_aperiodic_ratio(
    current_time: f32,
    frame_period_sec: f32,
    matrix: &MatrixF32,
    out: &mut [f32],
) {
    interpolate_matrix_frame(current_time, frame_period_sec, matrix, out, |value| {
        safe_aperiodicity(value)
    });
    for value in out {
        *value *= *value;
    }
}

fn interpolate_matrix_frame(
    current_time: f32,
    frame_period_sec: f32,
    matrix: &MatrixF32,
    out: &mut [f32],
    transform: impl Fn(f32) -> f32,
) {
    let position = current_time / frame_period_sec.max(SAFE_GUARD_MINIMUM);
    let floor = (position.floor() as usize).min(matrix.rows() - 1);
    let ceil = (position.ceil() as usize).min(matrix.rows() - 1);
    let interpolation = position - floor as f32;
    let floor_row = matrix.row(floor);
    let ceil_row = matrix.row(ceil);
    if floor == ceil {
        for (dst, &src) in out.iter_mut().zip(floor_row) {
            *dst = transform(src);
        }
    } else {
        for ((dst, &low), &high) in out.iter_mut().zip(floor_row).zip(ceil_row) {
            *dst = transform(low) * (1.0 - interpolation) + transform(high) * interpolation;
        }
    }
}

struct SynthesisResponseWorkspace {
    log_spectrum: Vec<f32>,
    minimum_phase: Vec<(f32, f32)>,
    shaped_spectrum: Vec<(f32, f32)>,
    noise_spectrum: Vec<(f32, f32)>,
    fft_real: Vec<f32>,
    fft_imag: Vec<f32>,
    shifted: Vec<f32>,
    noise_real: Vec<f32>,
    noise_imag: Vec<f32>,
}

fn resize_and_zero(values: &mut Vec<f32>, len: usize) {
    if values.len() != len {
        values.resize(len, 0.0);
    }
    values.fill(0.0);
}

fn resize_complex_and_zero(values: &mut Vec<(f32, f32)>, len: usize) {
    if values.len() != len {
        values.resize(len, (0.0, 0.0));
    }
    values.fill((0.0, 0.0));
}

impl SynthesisResponseWorkspace {
    fn new(fft_size: usize, bins: usize) -> Self {
        Self {
            log_spectrum: vec![0.0; bins],
            minimum_phase: vec![(0.0, 0.0); bins],
            shaped_spectrum: vec![(0.0, 0.0); bins],
            noise_spectrum: vec![(0.0, 0.0); bins],
            fft_real: vec![0.0; fft_size],
            fft_imag: vec![0.0; fft_size],
            shifted: vec![0.0; fft_size],
            noise_real: vec![0.0; fft_size],
            noise_imag: vec![0.0; fft_size],
        }
    }
}

fn get_periodic_response_into(
    spectrum: &[f32],
    aperiodic_ratio: &[f32],
    current_vuv: f32,
    fractional_time_shift: f32,
    fs: i32,
    dc_remover: &[f32],
    output: &mut [f32],
    workspace: &mut SynthesisResponseWorkspace,
) {
    let fft_size = (spectrum.len() - 1) * 2;
    debug_assert_eq!(output.len(), fft_size);
    if current_vuv <= 0.5 || aperiodic_ratio.first().copied().unwrap_or(1.0) > 0.999 {
        output.fill(0.0);
        return;
    }

    debug_assert_eq!(workspace.log_spectrum.len(), spectrum.len());
    for ((dst, &sp), &ap) in workspace
        .log_spectrum
        .iter_mut()
        .zip(spectrum)
        .zip(aperiodic_ratio)
    {
        *dst = (sp * (1.0 - ap) + SAFE_GUARD_MINIMUM)
            .max(SAFE_GUARD_MINIMUM)
            .ln()
            * 0.5;
    }
    minimum_phase_spectrum_into(
        &workspace.log_spectrum,
        &mut workspace.minimum_phase,
        &mut workspace.fft_real,
        &mut workspace.fft_imag,
    );
    let coefficient = 2.0 * PI * fractional_time_shift * fs as f32 / fft_size as f32;
    for (i, (real, imag)) in workspace.minimum_phase.iter_mut().enumerate() {
        let phase_real = (coefficient * i as f32).cos();
        let phase_imag = (1.0 - phase_real * phase_real).max(0.0).sqrt();
        let old_real = *real;
        let old_imag = *imag;
        *real = old_real * phase_real + old_imag * phase_imag;
        *imag = old_imag * phase_real - old_real * phase_imag;
    }

    inverse_real_fft_from_half_into(
        &workspace.minimum_phase,
        &mut workspace.shifted,
        &mut workspace.fft_real,
        &mut workspace.fft_imag,
    );
    fftshift_in_place(&mut workspace.shifted);
    let dc_component: f32 = workspace.shifted[fft_size / 2..].iter().sum();
    for ((dst, &value), &remover) in output.iter_mut().zip(&workspace.shifted).zip(dc_remover) {
        *dst = value - dc_component * remover;
    }
}

fn get_aperiodic_response_into(
    noise_size: usize,
    fft_size: usize,
    spectrum: &[f32],
    aperiodic_ratio: &[f32],
    current_vuv: f32,
    randn_state: &mut RandnState,
    output: &mut [f32],
    workspace: &mut SynthesisResponseWorkspace,
) {
    debug_assert_eq!(output.len(), fft_size);
    noise_spectrum_into(
        noise_size,
        fft_size,
        randn_state,
        &mut workspace.noise_spectrum,
        &mut workspace.noise_real,
        &mut workspace.noise_imag,
    );
    debug_assert_eq!(workspace.log_spectrum.len(), spectrum.len());
    for ((dst, &sp), &ap) in workspace
        .log_spectrum
        .iter_mut()
        .zip(spectrum)
        .zip(aperiodic_ratio)
    {
        let value = if current_vuv != 0.0 { sp * ap } else { sp };
        *dst = value.max(SAFE_GUARD_MINIMUM).ln() * 0.5;
    }
    minimum_phase_spectrum_into(
        &workspace.log_spectrum,
        &mut workspace.minimum_phase,
        &mut workspace.fft_real,
        &mut workspace.fft_imag,
    );
    debug_assert_eq!(workspace.shaped_spectrum.len(), fft_size / 2 + 1);
    for i in 0..=fft_size / 2 {
        workspace.shaped_spectrum[i].0 = workspace.minimum_phase[i].0
            * workspace.noise_spectrum[i].0
            - workspace.minimum_phase[i].1 * workspace.noise_spectrum[i].1;
        workspace.shaped_spectrum[i].1 = workspace.minimum_phase[i].0
            * workspace.noise_spectrum[i].1
            + workspace.minimum_phase[i].1 * workspace.noise_spectrum[i].0;
    }
    inverse_real_fft_from_half_into(
        &workspace.shaped_spectrum,
        output,
        &mut workspace.fft_real,
        &mut workspace.fft_imag,
    );
    fftshift_in_place(output);
}

fn noise_spectrum_into(
    noise_size: usize,
    fft_size: usize,
    randn_state: &mut RandnState,
    output: &mut Vec<(f32, f32)>,
    real: &mut Vec<f32>,
    imag: &mut Vec<f32>,
) {
    resize_and_zero(real, fft_size);
    let mut average = 0.0;
    for value in real.iter_mut().take(noise_size.min(fft_size)) {
        *value = randn_state.randn();
        average += *value;
    }
    average /= noise_size.max(1) as f32;
    for value in real.iter_mut().take(noise_size.min(fft_size)) {
        *value -= average;
    }
    resize_and_zero(imag, fft_size);
    fft(real, imag);
    debug_assert_eq!(output.len(), fft_size / 2 + 1);
    for i in 0..=fft_size / 2 {
        output[i] = (real[i], imag[i]);
    }
}

fn minimum_phase_spectrum_into(
    log_spectrum_half: &[f32],
    output: &mut Vec<(f32, f32)>,
    cepstrum_real: &mut Vec<f32>,
    cepstrum_imag: &mut Vec<f32>,
) {
    let fft_size = (log_spectrum_half.len() - 1) * 2;
    resize_and_zero(cepstrum_real, fft_size);
    resize_and_zero(cepstrum_imag, fft_size);
    cepstrum_real[..log_spectrum_half.len()].copy_from_slice(log_spectrum_half);
    for i in (fft_size / 2 + 1)..fft_size {
        cepstrum_real[i] = cepstrum_real[fft_size - i];
    }

    ifft(cepstrum_real, cepstrum_imag);
    for i in 1..fft_size / 2 {
        cepstrum_real[i] *= 2.0;
        cepstrum_imag[i] *= 2.0;
    }
    for i in (fft_size / 2 + 1)..fft_size {
        cepstrum_real[i] = 0.0;
        cepstrum_imag[i] = 0.0;
    }

    fft(cepstrum_real, cepstrum_imag);
    if output.len() != fft_size / 2 + 1 {
        resize_complex_and_zero(output, fft_size / 2 + 1);
    }
    for i in 0..=fft_size / 2 {
        let amplitude = cepstrum_real[i].exp();
        output[i] = (
            amplitude * cepstrum_imag[i].cos(),
            amplitude * cepstrum_imag[i].sin(),
        );
    }
}

fn inverse_real_fft_from_half_into(
    half_spectrum: &[(f32, f32)],
    output: &mut [f32],
    real: &mut Vec<f32>,
    imag: &mut Vec<f32>,
) {
    let fft_size = (half_spectrum.len() - 1) * 2;
    debug_assert_eq!(output.len(), fft_size);
    resize_and_zero(real, fft_size);
    resize_and_zero(imag, fft_size);
    for (i, &(r, im)) in half_spectrum.iter().enumerate() {
        real[i] = r;
        imag[i] = im;
    }
    for i in 1..fft_size / 2 {
        real[fft_size - i] = real[i];
        imag[fft_size - i] = -imag[i];
    }
    ifft(real, imag);
    output.copy_from_slice(real);
}

fn fftshift_in_place(values: &mut [f32]) {
    let half = values.len() / 2;
    values.rotate_left(half);
}

fn safe_aperiodicity(value: f32) -> f32 {
    value.clamp(0.001, 1.0 - SAFE_GUARD_MINIMUM)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::codec::interpolation_indices;

    fn indexed_time_base(
        f0: &[f32],
        fs: i32,
        frame_period_sec: f32,
        y_length: usize,
        lowest_f0: f32,
    ) -> SynthesisTimeBase {
        let mut coarse_time_axis = Vec::with_capacity(f0.len() + 1);
        let mut coarse_f0 = Vec::with_capacity(f0.len() + 1);
        let mut coarse_vuv = Vec::with_capacity(f0.len() + 1);

        for (i, &value) in f0.iter().enumerate() {
            coarse_time_axis.push(i as f32 * frame_period_sec);
            let voiced_f0 = if value < lowest_f0 { 0.0 } else { value };
            coarse_f0.push(voiced_f0);
            coarse_vuv.push(if voiced_f0 == 0.0 { 0.0 } else { 1.0 });
        }

        coarse_time_axis.push(f0.len() as f32 * frame_period_sec);
        let last_f0 = *coarse_f0.last().unwrap_or(&0.0);
        let prev_f0 = if coarse_f0.len() >= 2 {
            coarse_f0[coarse_f0.len() - 2]
        } else {
            last_f0
        };
        coarse_f0.push(last_f0 * 2.0 - prev_f0);
        let last_vuv = *coarse_vuv.last().unwrap_or(&0.0);
        let prev_vuv = if coarse_vuv.len() >= 2 {
            coarse_vuv[coarse_vuv.len() - 2]
        } else {
            last_vuv
        };
        coarse_vuv.push(last_vuv * 2.0 - prev_vuv);

        let mut interpolated_f0 = vec![0.0; y_length];
        let mut interpolated_vuv = vec![0.0; y_length];
        let time_targets: Vec<f32> = (0..y_length).map(|i| i as f32 / fs as f32).collect();
        let time_interpolation = interpolation_indices(&coarse_time_axis, &time_targets);
        for i in 0..y_length {
            interpolated_f0[i] = time_interpolation[i].interpolate(&coarse_f0);
            interpolated_vuv[i] = if time_interpolation[i].interpolate(&coarse_vuv) > 0.5 {
                1.0
            } else {
                0.0
            };
            if interpolated_vuv[i] == 0.0 {
                interpolated_f0[i] = DEFAULT_F0;
            }
        }

        let mut pulse_locations = Vec::new();
        let mut pulse_indices = Vec::new();
        let mut pulse_time_shifts = Vec::new();
        if y_length >= 2 {
            let mut previous_phase = (2.0 * PI * interpolated_f0[0] / fs as f32) % (2.0 * PI);
            for i in 1..y_length {
                let current_phase =
                    (previous_phase + 2.0 * PI * interpolated_f0[i] / fs as f32) % (2.0 * PI);
                if (current_phase - previous_phase).abs() > PI {
                    let y1 = previous_phase - 2.0 * PI;
                    let y2 = current_phase;
                    let crossing = if (y2 - y1).abs() > SAFE_GUARD_MINIMUM {
                        (-y1 / (y2 - y1)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    pulse_locations.push((i - 1) as f32 / fs as f32);
                    pulse_indices.push(i - 1);
                    pulse_time_shifts.push(crossing / fs as f32);
                }
                previous_phase = current_phase;
            }
        }

        SynthesisTimeBase {
            pulse_locations,
            pulse_indices,
            pulse_time_shifts,
            interpolated_vuv,
        }
    }

    #[test]
    fn direct_time_base_matches_indexed_interpolation_at_sensitive_boundaries() {
        let cases = [
            (44100, vec![220.0, 0.0, 246.94, 246.94, 440.0]),
            (48000, vec![95.0, 96.0, 0.0, 97.0, 330.0]),
        ];

        for (fs, f0) in cases {
            let fft_size = 1024;
            let frame_period_sec = 0.005;
            let y_length = ((f0.len() as f32 * frame_period_sec * fs as f32).floor()) as usize;
            let lowest_f0 = fs as f32 / fft_size as f32 + 1.0;
            let direct = synthesis_time_base(&f0, fs, frame_period_sec, y_length, lowest_f0);
            let indexed = indexed_time_base(&f0, fs, frame_period_sec, y_length, lowest_f0);

            assert_eq!(direct.pulse_indices, indexed.pulse_indices, "fs={fs}");
            assert_eq!(direct.interpolated_vuv, indexed.interpolated_vuv, "fs={fs}");
            assert_eq!(
                direct.pulse_locations.len(),
                indexed.pulse_locations.len(),
                "fs={fs}"
            );
            for (direct, indexed) in direct
                .pulse_locations
                .iter()
                .zip(indexed.pulse_locations.iter())
            {
                assert!((direct - indexed).abs() <= f32::EPSILON, "fs={}", fs);
            }
            for (direct, indexed) in direct
                .pulse_time_shifts
                .iter()
                .zip(indexed.pulse_time_shifts.iter())
            {
                assert!((direct - indexed).abs() <= f32::EPSILON, "fs={}", fs);
            }
        }
    }
}
