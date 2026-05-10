use std::f32::consts::PI;

use super::codec::interpolate_axis;
use super::fft::fft;
use super::random::RandnState;
use super::{
    get_number_of_aperiodicities, AperiodicityAnalysisConfig, MatrixF32, EPSILON_FOR_CHEAPTRICK,
    FLOOR_F0_D4C, FREQUENCY_INTERVAL, SAFE_GUARD_MINIMUM,
};

pub fn d4c(
    x: &[f32],
    fs: i32,
    temporal_positions: &[f32],
    f0: &[f32],
    fft_size: i32,
    config: &AperiodicityAnalysisConfig,
) -> MatrixF32 {
    let mut aperiodicity = MatrixF32::zeros(0, 0);
    d4c_into(
        x,
        fs,
        temporal_positions,
        f0,
        fft_size,
        config,
        &mut aperiodicity,
    );
    aperiodicity
}

fn get_coarse_aperiodicity(
    x: &[f32],
    fs: i32,
    temporal_position: f32,
    f0: f32,
    fft_size: usize,
    coarse_aperiodicity: &mut [f32],
    real_w: &mut [f32],
    imag_w: &mut [f32],
    real_dw: &mut [f32],
    imag_dw: &mut [f32],
) {
    let n_bands = coarse_aperiodicity.len();
    if f0 <= 40.0 {
        coarse_aperiodicity.fill(1.0);
        return;
    }

    debug_assert_eq!(real_w.len(), fft_size);
    debug_assert_eq!(imag_w.len(), fft_size);
    debug_assert_eq!(real_dw.len(), fft_size);
    debug_assert_eq!(imag_dw.len(), fft_size);
    real_w.fill(0.0);
    imag_w.fill(0.0);
    real_dw.fill(0.0);
    imag_dw.fill(0.0);

    let half_window_length = (2.0 * fs as f32 / f0).round() as isize;
    let base_index = (temporal_position * fs as f32).round() as isize;

    for i in 0..fft_size {
        let index = i as isize - fft_size as isize / 2 + base_index;
        if index >= 0 && index < x.len() as isize {
            let pos = (i as f32 - fft_size as f32 / 2.0) / half_window_length as f32;
            if pos.abs() < 1.0 {
                let x_win = PI * (pos + 1.0);
                let window = 0.42 - 0.5 * x_win.cos() + 0.08 * (2.0 * x_win).cos();
                let d_window = (0.5 * PI * x_win.sin() - 0.16 * PI * (2.0 * x_win).sin())
                    / half_window_length as f32;

                real_w[i] = x[index as usize] * window;
                real_dw[i] = x[index as usize] * d_window;
            }
        }
    }

    fft(real_w, imag_w);
    fft(real_dw, imag_dw);

    for i in 0..n_bands {
        let f_center = (i + 1) as f32 * FREQUENCY_INTERVAL;
        let mut energy_w = 0.0;
        let mut energy_dw = 0.0;

        let i_start = ((f_center - 1400.0) * fft_size as f32 / fs as f32)
            .round()
            .max(0.0) as usize;
        let i_end = ((f_center + 1400.0) * fft_size as f32 / fs as f32)
            .round()
            .min(fft_size as f32 / 2.0) as usize;

        for k in i_start..=i_end {
            let p_w = real_w[k] * real_w[k] + imag_w[k] * imag_w[k];
            let p_dw = real_dw[k] * real_dw[k] + imag_dw[k] * imag_dw[k];
            energy_w += p_w;
            energy_dw += p_dw;
        }

        if energy_w <= SAFE_GUARD_MINIMUM {
            coarse_aperiodicity[i] = 1.0;
        } else {
            let ratio = (energy_dw / energy_w).sqrt();
            let expected_ratio = PI / (half_window_length as f32 / fs as f32);
            let diff = (ratio / expected_ratio).min(1.0);

            let mut sum_energy = 0.0;
            let mut sum_f_energy = 0.0;
            for k in i_start..=i_end {
                let freq = k as f32 * fs as f32 / fft_size as f32;
                let energy = real_w[k] * real_w[k] + imag_w[k] * imag_w[k];
                sum_energy += energy;
                sum_f_energy += freq * energy;
            }
            let centroid = if sum_energy > SAFE_GUARD_MINIMUM {
                sum_f_energy / sum_energy
            } else {
                f_center
            };
            let centroid_diff = (centroid - f_center).abs() / 1500.0;

            let mut sum_gd = 0.0;
            for k in i_start..=i_end {
                let p_w = real_w[k] * real_w[k] + imag_w[k] * imag_w[k];
                if p_w > SAFE_GUARD_MINIMUM {
                    let gd = (real_w[k] * imag_dw[k] - imag_w[k] * real_dw[k]) / p_w;
                    sum_gd += gd * p_w;
                }
            }
            let avg_gd = if sum_energy > SAFE_GUARD_MINIMUM {
                sum_gd / sum_energy
            } else {
                0.0
            };
            let gd_diff = (avg_gd * fs as f32 / (2.0 * PI * fft_size as f32))
                .abs()
                .min(1.0);

            let linear_aperiodicity =
                ((1.0 - diff) * 0.4 + centroid_diff * 0.3 + gd_diff * 0.3).clamp(0.0001, 0.99);
            let db = 20.0 * linear_aperiodicity.log10();
            coarse_aperiodicity[i] = (db + (f0 - 100.0) / 64.0).min(0.0);
        }
    }
}

pub fn d4c_into(
    x: &[f32],
    fs: i32,
    temporal_positions: &[f32],
    f0: &[f32],
    fft_size: i32,
    _config: &AperiodicityAnalysisConfig,
    aperiodicity: &mut MatrixF32,
) {
    assert_eq!(temporal_positions.len(), f0.len());
    let cols = fft_size as usize / 2 + 1;
    aperiodicity.resize(f0.len(), cols);
    let fft_size_d4c =
        (2.0f32.powf(1.0 + (4.0 * fs as f32 / FLOOR_F0_D4C + 1.0).log2().floor())) as usize;

    let n_coarse = get_number_of_aperiodicities(fs) as usize;
    let mut coarse = vec![0.0; n_coarse];
    let mut coarse_extended = Vec::with_capacity(n_coarse + 2);
    let mut real_w = vec![0.0; fft_size_d4c];
    let mut imag_w = vec![0.0; fft_size_d4c];
    let mut real_dw = vec![0.0; fft_size_d4c];
    let mut imag_dw = vec![0.0; fft_size_d4c];

    let mut coarse_frequency_axis = Vec::with_capacity(n_coarse + 2);
    coarse_frequency_axis.push(0.0);
    for col in 0..n_coarse {
        coarse_frequency_axis.push(FREQUENCY_INTERVAL * (col + 1) as f32);
    }
    coarse_frequency_axis.push(fs as f32 / 2.0);

    for row in 0..f0.len() {
        if f0[row] <= 40.0 {
            aperiodicity.row_mut(row).fill(1.0 - SAFE_GUARD_MINIMUM);
            continue;
        }

        get_coarse_aperiodicity(
            x,
            fs,
            temporal_positions[row],
            f0[row].max(FLOOR_F0_D4C),
            fft_size_d4c,
            &mut coarse,
            &mut real_w,
            &mut imag_w,
            &mut real_dw,
            &mut imag_dw,
        );

        coarse_extended.clear();
        coarse_extended.push(-60.0);
        coarse_extended.extend_from_slice(&coarse);
        coarse_extended.push(-SAFE_GUARD_MINIMUM);

        for bin in 0..cols {
            let frequency = bin as f32 * fs as f32 / fft_size as f32;
            let db = interpolate_axis(&coarse_frequency_axis, &coarse_extended, frequency);
            aperiodicity.row_mut(row)[bin] = 10.0_f32.powf(db / 20.0);
        }
    }
}

pub fn d4c_from_spectrum(
    power_spectrum: &[f32],
    fft_size: i32,
    _fs: i32,
    f0: &[f32],
    temporal_positions: &[f32],
) -> MatrixF32 {
    assert_eq!(temporal_positions.len(), f0.len());
    let cols = fft_size as usize / 2 + 1;
    assert_eq!(power_spectrum.len(), f0.len().saturating_mul(cols));
    let mut aperiodicity = MatrixF32::zeros(f0.len(), cols);
    for row in 0..f0.len() {
        let input = &power_spectrum[row * cols..row * cols + cols];
        let mean = input.iter().sum::<f32>() / cols.max(1) as f32;
        let value = if mean <= SAFE_GUARD_MINIMUM { 1.0 } else { 0.5 };
        aperiodicity.row_mut(row).fill(value);
    }
    aperiodicity
}

fn matlab_round(x: f32) -> i32 {
    if x > 0.0 {
        (x + 0.5) as i32
    } else {
        (x - 0.5) as i32
    }
}

fn d4c_histc_f32(x: &[f32], edges: &[f32]) -> Vec<usize> {
    let mut index = vec![0; edges.len()];
    let mut count = 1;
    let x_length = x.len();
    let edges_length = edges.len();

    let mut i = 0;
    while i < edges_length {
        index[i] = 1;
        if edges[i] >= x[0] {
            break;
        }
        i += 1;
    }

    while i < edges_length {
        if edges[i] < x[count] {
            index[i] = count;
        } else {
            index[i] = count;
            count += 1;
            i -= 1;
        }
        i += 1;
        if count == x_length {
            break;
        }
    }

    if count == x_length {
        count -= 1;
        i += 1;
        while i < edges_length {
            index[i] = count;
            i += 1;
        }
    }

    index
}

fn d4c_interp1_f32(x: &[f32], y: &[f32], xi: &[f32]) -> Vec<f32> {
    let mut yi = vec![0.0; xi.len()];
    let mut h = vec![0.0; x.len() - 1];
    for i in 0..x.len() - 1 {
        h[i] = x[i + 1] - x[i];
    }
    let k = d4c_histc_f32(x, xi);
    for i in 0..xi.len() {
        let idx = k[i] - 1;
        let s = (xi[i] - x[idx]) / h[idx];
        yi[i] = y[idx] + s * (y[idx + 1] - y[idx]);
    }
    yi
}

fn d4c_interp1q_f32(x: f32, shift: f32, y: &[f32], xi: &[f32]) -> Vec<f32> {
    let mut delta_y = vec![0.0; y.len()];
    for i in 0..y.len().saturating_sub(1) {
        delta_y[i] = y[i + 1] - y[i];
    }
    if let Some(last) = delta_y.last_mut() {
        *last = 0.0;
    }

    xi.iter()
        .map(|&value| {
            let position = (value - x) / shift;
            let base = position as usize;
            let fraction = position - base as f32;
            y[base] + delta_y[base] * fraction
        })
        .collect()
}

fn d4c_nuttall_window_f32(length: usize) -> Vec<f32> {
    let mut window = vec![0.0; length];
    for i in 0..length {
        let x = 2.0 * PI * i as f32 / (length - 1) as f32;
        window[i] =
            0.355768 - 0.487396 * x.cos() + 0.144232 * (2.0 * x).cos() - 0.012604 * (3.0 * x).cos();
    }
    window
}

// --- D4C Core DSP Blocks (Stage 2) ---

fn d4c_dc_correction_f32(input: &[f32], f0: f32, fs: i32, fft_size: usize) -> Vec<f32> {
    let mut output = input.to_vec();
    let upper_limit = 2 + (f0 * fft_size as f32 / fs as f32) as usize;
    let upper_limit_replica = upper_limit.saturating_sub(1);
    let frequency_interval = fs as f32 / fft_size as f32;
    let low_frequency_axis: Vec<f32> = (0..upper_limit)
        .map(|i| i as f32 * frequency_interval)
        .collect();
    let replica = d4c_interp1q_f32(
        f0 - low_frequency_axis[0],
        -frequency_interval,
        &input[..(upper_limit + 1).min(input.len())],
        &low_frequency_axis[..upper_limit_replica.min(low_frequency_axis.len())],
    );

    for i in 0..upper_limit_replica.min(output.len()).min(replica.len()) {
        output[i] = input[i] + replica[i];
    }
    output
}

fn d4c_linear_smoothing_f32(input: &[f32], width: f32, fs: i32, fft_size: usize) -> Vec<f32> {
    let half_bins = fft_size / 2;
    let boundary = (width * fft_size as f32 / fs as f32) as usize + 1;
    let mirroring_len = half_bins + boundary * 2 + 1;
    let mut mirroring_spectrum = vec![0.0; mirroring_len];
    for i in 0..boundary {
        mirroring_spectrum[i] = input[boundary - i];
    }
    for i in boundary..(half_bins + boundary) {
        mirroring_spectrum[i] = input[i - boundary];
    }
    for i in (half_bins + boundary)..=(half_bins + boundary * 2) {
        mirroring_spectrum[i] = input[half_bins - (i - (half_bins + boundary))];
    }

    let discrete_frequency_interval = fs as f32 / fft_size as f32;
    let mut mirroring_segment = vec![0.0; mirroring_len];
    mirroring_segment[0] = mirroring_spectrum[0] * discrete_frequency_interval;
    for i in 1..mirroring_len {
        mirroring_segment[i] =
            mirroring_spectrum[i] * discrete_frequency_interval + mirroring_segment[i - 1];
    }

    let origin_of_mirroring_axis = -((boundary as f32) - 0.5) * discrete_frequency_interval;
    let frequency_axis: Vec<f32> = (0..=half_bins)
        .map(|i| i as f32 / fft_size as f32 * fs as f32 - width / 2.0)
        .collect();
    let low_levels = d4c_interp1q_f32(
        origin_of_mirroring_axis,
        discrete_frequency_interval,
        &mirroring_segment,
        &frequency_axis,
    );
    let high_frequency_axis: Vec<f32> = frequency_axis.iter().map(|value| value + width).collect();
    let high_levels = d4c_interp1q_f32(
        origin_of_mirroring_axis,
        discrete_frequency_interval,
        &mirroring_segment,
        &high_frequency_axis,
    );

    high_levels
        .iter()
        .zip(low_levels.iter())
        .map(|(&high, &low)| (high - low) / width)
        .collect()
}

fn d4c_get_windowed_waveform_f32(
    x: &[f32],
    fs: i32,
    current_f0: f32,
    current_position: f32,
    window_type: i32, // 1: Hanning, 2: Blackman
    window_length_ratio: f32,
    randn_state: &mut RandnState,
) -> (Vec<f32>, Vec<f32>) {
    let half_window_length = matlab_round(window_length_ratio * fs as f32 / current_f0 / 2.0);
    let mut window = vec![0.0; (half_window_length * 2 + 1) as usize];
    let mut waveform = vec![0.0; (half_window_length * 2 + 1) as usize];

    let origin = matlab_round(current_position * fs as f32 + 0.001);

    for i in -half_window_length..=half_window_length {
        let position = (2.0 * i as f32 / window_length_ratio) / fs as f32;
        let idx = (i + half_window_length) as usize;

        if window_type == 1 {
            window[idx] = 0.5 * (PI * position * current_f0).cos() + 0.5;
        } else {
            window[idx] = 0.42
                + 0.5 * (PI * position * current_f0).cos()
                + 0.08 * (2.0 * PI * position * current_f0).cos();
        }

        let x_idx = (origin + i).clamp(0, x.len() as i32 - 1) as usize;
        waveform[idx] = x[x_idx] * window[idx] + randn_state.randn() * SAFE_GUARD_MINIMUM;
    }

    let tmp_weight1: f32 = waveform.iter().sum();
    let tmp_weight2: f32 = window.iter().sum();
    let weighting_coefficient = tmp_weight1 / tmp_weight2.max(SAFE_GUARD_MINIMUM);

    for i in 0..waveform.len() {
        waveform[i] -= window[i] * weighting_coefficient;
    }

    (waveform, window)
}

// --- D4C Analysis Blocks (Stage 3) ---

fn d4c_get_centroid_f32(
    x: &[f32],
    fs: i32,
    current_f0: f32,
    fft_size: usize,
    current_position: f32,
    randn_state: &mut RandnState,
) -> Vec<f32> {
    let (mut waveform, _) =
        d4c_get_windowed_waveform_f32(x, fs, current_f0, current_position, 2, 4.0, randn_state);

    let power: f32 = waveform.iter().map(|&v| v * v).sum();
    let norm = power.sqrt().max(SAFE_GUARD_MINIMUM);
    for v in &mut waveform {
        *v /= norm;
    }

    let mut real = vec![0.0; fft_size];
    let len = waveform.len().min(fft_size);
    real[..len].copy_from_slice(&waveform[..len]);
    let mut imag = vec![0.0; fft_size];
    fft(&mut real, &mut imag);

    let mut spectrum_x = vec![(0.0, 0.0); fft_size / 2 + 1];
    for i in 0..=fft_size / 2 {
        spectrum_x[i] = (real[i], imag[i]);
    }

    let mut real_y = vec![0.0; fft_size];
    for i in 0..len {
        real_y[i] = waveform[i] * (i as f32 + 1.0);
    }
    let mut imag_y = vec![0.0; fft_size];
    fft(&mut real_y, &mut imag_y);

    let mut centroid = vec![0.0; fft_size / 2 + 1];
    for i in 0..=fft_size / 2 {
        centroid[i] = real_y[i] * spectrum_x[i].0 + imag_y[i] * spectrum_x[i].1;
    }

    centroid
}

fn d4c_get_static_centroid_f32(
    x: &[f32],
    fs: i32,
    current_f0: f32,
    fft_size: usize,
    current_position: f32,
    randn_state: &mut RandnState,
) -> Vec<f32> {
    let centroid1 = d4c_get_centroid_f32(
        x,
        fs,
        current_f0,
        fft_size,
        current_position - 0.25 / current_f0,
        randn_state,
    );
    let centroid2 = d4c_get_centroid_f32(
        x,
        fs,
        current_f0,
        fft_size,
        current_position + 0.25 / current_f0,
        randn_state,
    );

    let mut static_centroid = vec![0.0; fft_size / 2 + 1];
    for i in 0..=fft_size / 2 {
        static_centroid[i] = centroid1[i] + centroid2[i];
    }

    d4c_dc_correction_f32(&static_centroid, current_f0, fs, fft_size)
}

fn d4c_get_smoothed_power_spectrum_f32(
    x: &[f32],
    fs: i32,
    current_f0: f32,
    fft_size: usize,
    current_position: f32,
    randn_state: &mut RandnState,
) -> Vec<f32> {
    let (waveform, _) =
        d4c_get_windowed_waveform_f32(x, fs, current_f0, current_position, 1, 4.0, randn_state);

    let mut real = vec![0.0; fft_size];
    let len = waveform.len().min(fft_size);
    real[..len].copy_from_slice(&waveform[..len]);
    let mut imag = vec![0.0; fft_size];
    fft(&mut real, &mut imag);

    let mut smoothed_power_spectrum = vec![0.0; fft_size / 2 + 1];
    for i in 0..=fft_size / 2 {
        smoothed_power_spectrum[i] = real[i] * real[i] + imag[i] * imag[i];
    }

    let corrected = d4c_dc_correction_f32(&smoothed_power_spectrum, current_f0, fs, fft_size);
    d4c_linear_smoothing_f32(&corrected, current_f0, fs, fft_size)
}

fn d4c_get_static_group_delay_f32(
    static_centroid: &[f32],
    smoothed_power_spectrum: &[f32],
    fs: i32,
    f0: f32,
    fft_size: usize,
) -> Vec<f32> {
    let mut static_group_delay = vec![0.0; fft_size / 2 + 1];
    for i in 0..=fft_size / 2 {
        static_group_delay[i] =
            static_centroid[i] / smoothed_power_spectrum[i].max(EPSILON_FOR_CHEAPTRICK);
    }

    let static_group_delay = d4c_linear_smoothing_f32(&static_group_delay, f0 / 2.0, fs, fft_size);
    let smoothed_group_delay = d4c_linear_smoothing_f32(&static_group_delay, f0, fs, fft_size);

    static_group_delay
        .iter()
        .zip(smoothed_group_delay.iter())
        .map(|(&a, &b)| a - b)
        .collect()
}

// --- D4C Core Logic (Stage 4) ---

fn d4c_love_train_sub_f32(
    x: &[f32],
    fs: i32,
    current_f0: f32,
    current_position: f32,
    fft_size: usize,
    boundary0: usize,
    boundary1: usize,
    boundary2: usize,
    randn_state: &mut RandnState,
) -> f32 {
    let (waveform, _) =
        d4c_get_windowed_waveform_f32(x, fs, current_f0, current_position, 2, 3.0, randn_state);

    let mut real = vec![0.0; fft_size];
    let len = waveform.len().min(fft_size);
    real[..len].copy_from_slice(&waveform[..len]);
    let mut imag = vec![0.0; fft_size];
    fft(&mut real, &mut imag);

    let mut cumulative_power = 0.0;
    let mut power_at_boundary1 = 0.0;
    let mut power_at_boundary2 = 0.0;

    for i in 0..=boundary2.min(fft_size / 2) {
        let power = if i <= boundary0 {
            0.0
        } else {
            real[i] * real[i] + imag[i] * imag[i]
        };
        cumulative_power += power;
        if i == boundary1 {
            power_at_boundary1 = cumulative_power;
        }
        if i == boundary2 {
            power_at_boundary2 = cumulative_power;
        }
    }

    power_at_boundary1 / power_at_boundary2.max(EPSILON_FOR_CHEAPTRICK)
}

fn d4c_get_coarse_aperiodicity_with_window_f32(
    static_group_delay: &[f32],
    fs: i32,
    fft_size: usize,
    number_of_aperiodicities: usize,
    window: &[f32],
) -> Vec<f32> {
    let mut coarse_aperiodicity = vec![0.0; number_of_aperiodicities];
    let window_length = window.len();
    let half_window_length = window_length / 2;
    let boundary = matlab_round(fft_size as f32 * 8.0 / window_length as f32) as usize;
    let mut waveform = vec![0.0; fft_size];
    let mut power_spectrum = vec![0.0; fft_size / 2 + 1];

    for i in 0..number_of_aperiodicities {
        let center = ((i + 1) as f32 * FREQUENCY_INTERVAL * fft_size as f32 / fs as f32) as usize;
        waveform.fill(0.0);
        for j in 0..window_length {
            waveform[j] = static_group_delay[center - half_window_length + j] * window[j];
        }

        let mut imag = vec![0.0; fft_size];
        fft(&mut waveform, &mut imag);
        for j in 0..power_spectrum.len() {
            power_spectrum[j] = waveform[j] * waveform[j] + imag[j] * imag[j];
        }
        power_spectrum.sort_by(|a, b| a.partial_cmp(b).unwrap());

        for j in 1..=fft_size / 2 {
            power_spectrum[j] += power_spectrum[j - 1];
        }

        let p_sum_num = power_spectrum[fft_size / 2 - boundary - 1];
        let p_sum_den = power_spectrum[fft_size / 2];

        coarse_aperiodicity[i] = 10.0
            * (p_sum_num.max(EPSILON_FOR_CHEAPTRICK) / p_sum_den.max(EPSILON_FOR_CHEAPTRICK))
                .log10();
    }

    coarse_aperiodicity
}

fn d4c_general_body_f32(
    x: &[f32],
    fs: i32,
    current_f0: f32,
    fft_size: usize,
    current_position: f32,
    number_of_aperiodicities: usize,
    window: &[f32],
    randn_state: &mut RandnState,
) -> Vec<f32> {
    let static_centroid =
        d4c_get_static_centroid_f32(x, fs, current_f0, fft_size, current_position, randn_state);
    let smoothed_power_spectrum = d4c_get_smoothed_power_spectrum_f32(
        x,
        fs,
        current_f0,
        fft_size,
        current_position,
        randn_state,
    );
    let static_group_delay = d4c_get_static_group_delay_f32(
        &static_centroid,
        &smoothed_power_spectrum,
        fs,
        current_f0,
        fft_size,
    );

    d4c_get_coarse_aperiodicity_with_window_f32(
        &static_group_delay,
        fs,
        fft_size,
        number_of_aperiodicities,
        window,
    )
}
