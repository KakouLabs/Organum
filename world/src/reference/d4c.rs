use std::f64::consts::PI;

pub const K_MY_SAFE_GUARD_MINIMUM: f64 = 1.0e-12;
pub const K_EPS: f64 = 2.2204460492503131e-16;
pub const K_FREQUENCY_INTERVAL: f64 = 3000.0;
pub const K_UPPER_LIMIT: f64 = 15000.0;
pub const K_THRESHOLD: f64 = 0.85;
pub const K_FLOOR_F0_D4C: f64 = 47.0;
pub const K_LOG2: f64 = 0.69314718055994529;

#[derive(Debug, Clone, Default)]
pub struct D4CDebugFrame {
    pub aperiodicity0: f64,
    pub static_centroid: Vec<f64>,
    pub smoothed_power_spectrum: Vec<f64>,
    pub static_group_delay: Vec<f64>,
    pub coarse_aperiodicity: Vec<f64>,
}

pub fn matlab_round(x: f64) -> i32 {
    if x > 0.0 {
        (x + 0.5) as i32
    } else {
        (x - 0.5) as i32
    }
}

pub fn nuttall_window(length: usize) -> Vec<f64> {
    let mut window = vec![0.0; length];
    for i in 0..length {
        let x = 2.0 * PI * i as f64 / (length - 1) as f64;
        window[i] =
            0.355768 - 0.487396 * x.cos() + 0.144232 * (2.0 * x).cos() - 0.012604 * (3.0 * x).cos();
    }
    window
}

fn bit_reverse(n: usize, bits: usize) -> usize {
    let mut reversed = 0;
    let mut n = n;
    for _ in 0..bits {
        reversed = (reversed << 1) | (n & 1);
        n >>= 1;
    }
    reversed
}

fn fft(real: &mut [f64], imag: &mut [f64]) {
    let n = real.len();
    let bits = n.trailing_zeros() as usize;
    for i in 0..n {
        let j = bit_reverse(i, bits);
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
    }

    let mut len = 2;
    while len <= n {
        let angle = -2.0 * PI / len as f64;
        let w_len_real = angle.cos();
        let w_len_imag = angle.sin();
        for i in (0..n).step_by(len) {
            let mut w_real = 1.0;
            let mut w_imag = 0.0;
            for j in 0..len / 2 {
                let u_real = real[i + j];
                let u_imag = imag[i + j];
                let v_real = real[i + j + len / 2] * w_real - imag[i + j + len / 2] * w_imag;
                let v_imag = real[i + j + len / 2] * w_imag + imag[i + j + len / 2] * w_real;
                real[i + j] = u_real + v_real;
                imag[i + j] = u_imag + v_imag;
                real[i + j + len / 2] = u_real - v_real;
                imag[i + j + len / 2] = u_imag - v_imag;
                let tmp = w_real * w_len_real - w_imag * w_len_imag;
                w_imag = w_real * w_len_imag + w_imag * w_len_real;
                w_real = tmp;
            }
        }
        len <<= 1;
    }
}

fn fft_forward_real(waveform: &[f64], fft_size: usize) -> Vec<(f64, f64)> {
    let mut real = vec![0.0; fft_size];
    real[..waveform.len().min(fft_size)].copy_from_slice(&waveform[..waveform.len().min(fft_size)]);
    let mut imag = vec![0.0; fft_size];
    fft(&mut real, &mut imag);
    (0..=fft_size / 2).map(|i| (real[i], imag[i])).collect()
}

pub fn d4c_love_train_sub(
    x: &[f64],
    fs: i32,
    current_f0: f64,
    current_position: f64,
    fft_size: usize,
    boundary0: usize,
    boundary1: usize,
    boundary2: usize,
    randn_state: &mut RandnState,
) -> f64 {
    let (waveform, _) =
        get_windowed_waveform(x, fs, current_f0, current_position, 2, 3.0, randn_state);
    let spectrum = fft_forward_real(&waveform, fft_size);

    let mut cumulative_power = 0.0;
    let mut power_at_boundary1 = 0.0;
    let mut power_at_boundary2 = 0.0;

    for i in 0..=boundary2.min(fft_size / 2) {
        let power = if i <= boundary0 {
            0.0
        } else {
            spectrum[i].0 * spectrum[i].0 + spectrum[i].1 * spectrum[i].1
        };
        cumulative_power += power;
        if i == boundary1 {
            power_at_boundary1 = cumulative_power;
        }
        if i == boundary2 {
            power_at_boundary2 = cumulative_power;
        }
    }

    power_at_boundary1 / power_at_boundary2.max(K_EPS)
}

pub fn dc_correction(input: &[f64], f0: f64, fs: i32, fft_size: usize) -> Vec<f64> {
    let mut output = input.to_vec();
    let upper_limit = 2 + (f0 * fft_size as f64 / fs as f64) as usize;
    let upper_limit_replica = upper_limit.saturating_sub(1);
    let frequency_interval = fs as f64 / fft_size as f64;
    let low_frequency_axis: Vec<f64> = (0..upper_limit)
        .map(|i| i as f64 * frequency_interval)
        .collect();
    let replica = interp1q(
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

pub fn linear_smoothing(input: &[f64], width: f64, fs: i32, fft_size: usize) -> Vec<f64> {
    let half_bins = fft_size / 2;
    let boundary = (width * fft_size as f64 / fs as f64) as usize + 1;
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

    let discrete_frequency_interval = fs as f64 / fft_size as f64;
    let mut mirroring_segment = vec![0.0; mirroring_len];
    mirroring_segment[0] = mirroring_spectrum[0] * discrete_frequency_interval;
    for i in 1..mirroring_len {
        mirroring_segment[i] =
            mirroring_spectrum[i] * discrete_frequency_interval + mirroring_segment[i - 1];
    }

    let origin_of_mirroring_axis = -((boundary as f64) - 0.5) * discrete_frequency_interval;
    let frequency_axis: Vec<f64> = (0..=half_bins)
        .map(|i| i as f64 / fft_size as f64 * fs as f64 - width / 2.0)
        .collect();
    let low_levels = interp1q(
        origin_of_mirroring_axis,
        discrete_frequency_interval,
        &mirroring_segment,
        &frequency_axis,
    );
    let high_frequency_axis: Vec<f64> = frequency_axis.iter().map(|value| value + width).collect();
    let high_levels = interp1q(
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

pub struct RandnState {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
}

impl RandnState {
    pub fn new() -> Self {
        Self {
            x: 123456789,
            y: 362436069,
            z: 521288629,
            w: 88675123,
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        let t = self.x ^ (self.x << 11);
        self.x = self.y;
        self.y = self.z;
        self.z = self.w;
        self.w = (self.w ^ (self.w >> 19)) ^ (t ^ (t >> 8));
        self.w
    }

    pub fn randn(&mut self) -> f64 {
        let mut tmp = self.next_u32() >> 4;
        for _ in 0..11 {
            tmp += self.next_u32() >> 4;
        }
        tmp as f64 / 268435456.0 - 6.0
    }
}

pub fn get_windowed_waveform(
    x: &[f64],
    fs: i32,
    current_f0: f64,
    current_position: f64,
    window_type: i32, // 1: Hanning, 2: Blackman
    window_length_ratio: f64,
    randn_state: &mut RandnState,
) -> (Vec<f64>, Vec<f64>) {
    let half_window_length = matlab_round(window_length_ratio * fs as f64 / current_f0 / 2.0);
    let mut window = vec![0.0; (half_window_length * 2 + 1) as usize];
    let mut waveform = vec![0.0; (half_window_length * 2 + 1) as usize];

    let origin = matlab_round(current_position * fs as f64 + 0.001);

    for i in -half_window_length..=half_window_length {
        let position = (2.0 * i as f64 / window_length_ratio) / fs as f64;
        let idx = (i + half_window_length) as usize;

        if window_type == 1 {
            window[idx] = 0.5 * (PI * position * current_f0).cos() + 0.5;
        } else {
            window[idx] = 0.42
                + 0.5 * (PI * position * current_f0).cos()
                + 0.08 * (2.0 * PI * position * current_f0).cos();
        }

        let x_idx = (origin + i).clamp(0, x.len() as i32 - 1) as usize;
        waveform[idx] = x[x_idx] * window[idx] + randn_state.randn() * K_MY_SAFE_GUARD_MINIMUM;
    }

    let tmp_weight1: f64 = waveform.iter().sum();
    let tmp_weight2: f64 = window.iter().sum();
    let weighting_coefficient = tmp_weight1 / tmp_weight2;

    for i in 0..waveform.len() {
        waveform[i] -= window[i] * weighting_coefficient;
    }

    (waveform, window)
}

pub fn get_centroid(
    x: &[f64],
    fs: i32,
    current_f0: f64,
    fft_size: usize,
    current_position: f64,
    randn_state: &mut RandnState,
) -> Vec<f64> {
    let (mut waveform, _) =
        get_windowed_waveform(x, fs, current_f0, current_position, 2, 4.0, randn_state);

    let power: f64 = waveform.iter().map(|&v| v * v).sum();
    let norm = power.sqrt().max(K_MY_SAFE_GUARD_MINIMUM);
    for v in &mut waveform {
        *v /= norm;
    }

    let mut padded_waveform = vec![0.0; fft_size];
    padded_waveform[..waveform.len().min(fft_size)]
        .copy_from_slice(&waveform[..waveform.len().min(fft_size)]);

    let spectrum_x = fft_forward_real(&padded_waveform, fft_size);

    for i in 0..waveform.len().min(fft_size) {
        padded_waveform[i] *= i as f64 + 1.0;
    }
    if waveform.len() < fft_size {
        for i in waveform.len()..fft_size {
            padded_waveform[i] = 0.0;
        }
    }

    let spectrum_y = fft_forward_real(&padded_waveform, fft_size);

    let mut centroid = vec![0.0; fft_size / 2 + 1];
    for i in 0..=fft_size / 2 {
        centroid[i] = spectrum_y[i].0 * spectrum_x[i].0 + spectrum_y[i].1 * spectrum_x[i].1;
    }

    centroid
}

pub fn get_static_centroid(
    x: &[f64],
    fs: i32,
    current_f0: f64,
    fft_size: usize,
    current_position: f64,
    randn_state: &mut RandnState,
) -> Vec<f64> {
    let centroid1 = get_centroid(
        x,
        fs,
        current_f0,
        fft_size,
        current_position - 0.25 / current_f0,
        randn_state,
    );
    let centroid2 = get_centroid(
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

    dc_correction(&static_centroid, current_f0, fs, fft_size)
}

pub fn get_smoothed_power_spectrum(
    x: &[f64],
    fs: i32,
    current_f0: f64,
    fft_size: usize,
    current_position: f64,
    randn_state: &mut RandnState,
) -> Vec<f64> {
    let (waveform, _) =
        get_windowed_waveform(x, fs, current_f0, current_position, 1, 4.0, randn_state);

    let mut padded_waveform = vec![0.0; fft_size];
    padded_waveform[..waveform.len().min(fft_size)]
        .copy_from_slice(&waveform[..waveform.len().min(fft_size)]);

    let spectrum = fft_forward_real(&padded_waveform, fft_size);
    let mut smoothed_power_spectrum = vec![0.0; fft_size / 2 + 1];
    for i in 0..=fft_size / 2 {
        smoothed_power_spectrum[i] = spectrum[i].0 * spectrum[i].0 + spectrum[i].1 * spectrum[i].1;
    }

    let corrected = dc_correction(&smoothed_power_spectrum, current_f0, fs, fft_size);
    linear_smoothing(&corrected, current_f0, fs, fft_size)
}

pub fn get_static_group_delay(
    static_centroid: &[f64],
    smoothed_power_spectrum: &[f64],
    fs: i32,
    f0: f64,
    fft_size: usize,
) -> Vec<f64> {
    let mut static_group_delay = vec![0.0; fft_size / 2 + 1];
    for i in 0..=fft_size / 2 {
        static_group_delay[i] = static_centroid[i] / smoothed_power_spectrum[i].max(K_EPS);
    }

    let static_group_delay = linear_smoothing(&static_group_delay, f0 / 2.0, fs, fft_size);
    let smoothed_group_delay = linear_smoothing(&static_group_delay, f0, fs, fft_size);

    static_group_delay
        .iter()
        .zip(smoothed_group_delay.iter())
        .map(|(&a, &b)| a - b)
        .collect()
}

pub fn get_coarse_aperiodicity(
    static_group_delay: &[f64],
    fs: i32,
    fft_size: usize,
    number_of_aperiodicities: usize,
) -> Vec<f64> {
    let window_length = (K_FREQUENCY_INTERVAL * fft_size as f64 / fs as f64) as usize * 2 + 1;
    let window = nuttall_window(window_length);
    get_coarse_aperiodicity_with_window(
        static_group_delay,
        fs,
        fft_size,
        number_of_aperiodicities,
        &window,
    )
}

pub fn get_coarse_aperiodicity_with_window(
    static_group_delay: &[f64],
    fs: i32,
    fft_size: usize,
    number_of_aperiodicities: usize,
    window: &[f64],
) -> Vec<f64> {
    let mut coarse_aperiodicity = vec![0.0; number_of_aperiodicities];
    let window_length = window.len();
    let half_window_length = window_length / 2;
    let boundary = matlab_round(fft_size as f64 * 8.0 / window_length as f64) as usize;
    let mut waveform = vec![0.0; fft_size];
    let mut power_spectrum = vec![0.0; fft_size / 2 + 1];

    for i in 0..number_of_aperiodicities {
        let center = ((i + 1) as f64 * K_FREQUENCY_INTERVAL * fft_size as f64 / fs as f64) as usize;
        for j in 0..window_length {
            waveform[j] = static_group_delay[center - half_window_length + j] * window[j];
        }

        let spectrum = fft_forward_real(&waveform, fft_size);
        for j in 0..power_spectrum.len() {
            power_spectrum[j] = spectrum[j].0 * spectrum[j].0 + spectrum[j].1 * spectrum[j].1;
        }
        power_spectrum.sort_by(|a, b| a.partial_cmp(b).unwrap());

        for j in 1..=fft_size / 2 {
            power_spectrum[j] += power_spectrum[j - 1];
        }

        let p_sum_num = power_spectrum[fft_size / 2 - boundary - 1];
        let p_sum_den = power_spectrum[fft_size / 2];

        coarse_aperiodicity[i] = 10.0 * (p_sum_num.max(K_EPS) / p_sum_den.max(K_EPS)).log10();
    }

    coarse_aperiodicity
}

pub fn d4c_debug_ref(
    x: &[f64],
    fs: i32,
    current_position: f64,
    current_f0: f64,
    fft_size: usize,
) -> D4CDebugFrame {
    let mut randn_state = RandnState::new();

    let boundary0 = (100.0 * fft_size as f64 / fs as f64).ceil() as usize;
    let boundary1 = (4000.0 * fft_size as f64 / fs as f64).ceil() as usize;
    let boundary2 = (7900.0 * fft_size as f64 / fs as f64).ceil() as usize;

    let aperiodicity0 = d4c_love_train_sub(
        x,
        fs,
        current_f0.max(40.0),
        current_position,
        fft_size,
        boundary0,
        boundary1,
        boundary2,
        &mut randn_state,
    );

    let static_centroid = get_static_centroid(
        x,
        fs,
        current_f0,
        fft_size,
        current_position,
        &mut randn_state,
    );
    let smoothed_power_spectrum = get_smoothed_power_spectrum(
        x,
        fs,
        current_f0,
        fft_size,
        current_position,
        &mut randn_state,
    );
    let static_group_delay = get_static_group_delay(
        &static_centroid,
        &smoothed_power_spectrum,
        fs,
        current_f0,
        fft_size,
    );

    let number_of_aperiodicities = ((K_UPPER_LIMIT.min(fs as f64 / 2.0 - K_FREQUENCY_INTERVAL))
        / K_FREQUENCY_INTERVAL)
        .floor() as usize;
    let coarse_aperiodicity =
        get_coarse_aperiodicity(&static_group_delay, fs, fft_size, number_of_aperiodicities);

    D4CDebugFrame {
        aperiodicity0,
        static_centroid,
        smoothed_power_spectrum,
        static_group_delay,
        coarse_aperiodicity,
    }
}

pub fn d4c_general_body(
    x: &[f64],
    fs: i32,
    current_f0: f64,
    fft_size: usize,
    current_position: f64,
    number_of_aperiodicities: usize,
    window: &[f64],
    randn_state: &mut RandnState,
) -> Vec<f64> {
    let static_centroid =
        get_static_centroid(x, fs, current_f0, fft_size, current_position, randn_state);
    let smoothed_power_spectrum =
        get_smoothed_power_spectrum(x, fs, current_f0, fft_size, current_position, randn_state);
    let static_group_delay = get_static_group_delay(
        &static_centroid,
        &smoothed_power_spectrum,
        fs,
        current_f0,
        fft_size,
    );

    get_coarse_aperiodicity_with_window(
        &static_group_delay,
        fs,
        fft_size,
        number_of_aperiodicities,
        window,
    )
}

pub fn d4c_f64_reference(
    x: &[f64],
    fs: i32,
    temporal_positions: &[f64],
    f0: &[f64],
    fft_size: i32,
    threshold: f64,
) -> Vec<Vec<f64>> {
    let f0_length = f0.len();
    let mut aperiodicity =
        vec![vec![1.0 - K_MY_SAFE_GUARD_MINIMUM; (fft_size / 2 + 1) as usize]; f0_length];

    let fft_size_d4c =
        (2.0f64.powf(1.0 + (4.0 * fs as f64 / K_FLOOR_F0_D4C + 1.0).log2().floor())) as usize;
    let number_of_aperiodicities = ((K_UPPER_LIMIT.min(fs as f64 / 2.0 - K_FREQUENCY_INTERVAL))
        / K_FREQUENCY_INTERVAL)
        .floor() as usize;

    let mut randn_state = RandnState::new();

    let window_length = (K_FREQUENCY_INTERVAL * fft_size_d4c as f64 / fs as f64) as usize * 2 + 1;
    let window = nuttall_window(window_length);

    // D4C Love Train
    let mut aperiodicity0 = vec![0.0; f0_length];
    let lowest_f0_love_train = 40.0;
    let fft_size_love_train = (2.0f64.powf(
        1.0 + (3.0 * fs as f64 / lowest_f0_love_train + 1.0)
            .log2()
            .floor(),
    )) as usize;
    let boundary0 = (100.0 * fft_size_love_train as f64 / fs as f64).ceil() as usize;
    let boundary1 = (4000.0 * fft_size_love_train as f64 / fs as f64).ceil() as usize;
    let boundary2 = (7900.0 * fft_size_love_train as f64 / fs as f64).ceil() as usize;

    for i in 0..f0_length {
        if f0[i] == 0.0 {
            aperiodicity0[i] = 0.0;
            continue;
        }
        aperiodicity0[i] = d4c_love_train_sub(
            x,
            fs,
            f0[i].max(lowest_f0_love_train),
            temporal_positions[i],
            fft_size_love_train,
            boundary0,
            boundary1,
            boundary2,
            &mut randn_state,
        );
    }

    let mut coarse_aperiodicity = vec![0.0; number_of_aperiodicities + 2];
    coarse_aperiodicity[0] = -60.0;
    coarse_aperiodicity[number_of_aperiodicities + 1] = -K_MY_SAFE_GUARD_MINIMUM;
    let mut coarse_frequency_axis = vec![0.0; number_of_aperiodicities + 2];
    for i in 0..=number_of_aperiodicities {
        coarse_frequency_axis[i] = i as f64 * K_FREQUENCY_INTERVAL;
    }
    coarse_frequency_axis[number_of_aperiodicities + 1] = fs as f64 / 2.0;

    let mut frequency_axis = vec![0.0; (fft_size / 2 + 1) as usize];
    for i in 0..=(fft_size / 2) as usize {
        frequency_axis[i] = i as f64 * fs as f64 / fft_size as f64;
    }

    for i in 0..f0_length {
        if f0[i] == 0.0 || aperiodicity0[i] <= threshold {
            continue;
        }

        let mut coarse_body = d4c_general_body(
            x,
            fs,
            f0[i].max(K_FLOOR_F0_D4C),
            fft_size_d4c,
            temporal_positions[i],
            number_of_aperiodicities,
            &window,
            &mut randn_state,
        );

        for val in &mut coarse_body {
            *val = (*val + (f0[i] - 100.0) / 50.0).min(0.0);
        }

        coarse_aperiodicity[1..=number_of_aperiodicities].copy_from_slice(&coarse_body);

        let mut ap_i = interp1(
            &coarse_frequency_axis,
            &coarse_aperiodicity,
            &frequency_axis,
        );
        for val in &mut ap_i {
            *val = 10.0f64.powf(*val / 20.0);
        }
        aperiodicity[i] = ap_i;
    }

    aperiodicity
}

pub fn histc(x: &[f64], edges: &[f64]) -> Vec<usize> {
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

pub fn interp1(x: &[f64], y: &[f64], xi: &[f64]) -> Vec<f64> {
    let mut yi = vec![0.0; xi.len()];
    let mut h = vec![0.0; x.len() - 1];
    for i in 0..x.len() - 1 {
        h[i] = x[i + 1] - x[i];
    }
    let k = histc(x, xi);
    for i in 0..xi.len() {
        let idx = k[i] - 1;
        let s = (xi[i] - x[idx]) / h[idx];
        yi[i] = y[idx] + s * (y[idx + 1] - y[idx]);
    }
    yi
}

pub fn interp1q(x: f64, shift: f64, y: &[f64], xi: &[f64]) -> Vec<f64> {
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
            let fraction = position - base as f64;
            y[base] + delta_y[base] * fraction
        })
        .collect()
}
