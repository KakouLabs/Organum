use std::{cell::RefCell, collections::HashMap, f32::consts::PI, rc::Rc};

struct FftStagePlan {
    len: usize,
    twiddles: Vec<(f32, f32)>,
}

struct FftPlan {
    swaps: Vec<(usize, usize)>,
    stages: Vec<FftStagePlan>,
}

impl FftPlan {
    fn new(n: usize) -> Self {
        let mut swaps = Vec::new();
        let mut j = 0usize;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                swaps.push((i, j));
            }
        }

        let mut stages = Vec::new();
        let mut len = 2;
        while len <= n {
            let angle = -2.0 * PI / len as f32;
            let w_len_real = angle.cos();
            let w_len_imag = angle.sin();
            let mut twiddles = Vec::with_capacity(len / 2);
            let mut w_real = 1.0;
            let mut w_imag = 0.0;
            for _ in 0..len / 2 {
                twiddles.push((w_real, w_imag));
                let tmp = w_real * w_len_real - w_imag * w_len_imag;
                w_imag = w_real * w_len_imag + w_imag * w_len_real;
                w_real = tmp;
            }
            stages.push(FftStagePlan { len, twiddles });
            len <<= 1;
        }

        Self { swaps, stages }
    }
}

thread_local! {
    static FFT_PLAN_CACHE: RefCell<HashMap<usize, Rc<FftPlan>>> = RefCell::new(HashMap::new());
}

fn fft_plan(n: usize) -> Rc<FftPlan> {
    FFT_PLAN_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache
            .entry(n)
            .or_insert_with(|| Rc::new(FftPlan::new(n)))
            .clone()
    })
}

pub(super) fn fft(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    assert!(
        n.is_power_of_two(),
        "FFT size must be a power of two, got {}",
        n
    );
    let plan = fft_plan(n);
    for &(i, j) in &plan.swaps {
        real.swap(i, j);
        imag.swap(i, j);
    }

    for stage in &plan.stages {
        let len = stage.len;
        for i in (0..n).step_by(len) {
            for (j, &(w_real, w_imag)) in stage.twiddles.iter().enumerate() {
                let u_real = real[i + j];
                let u_imag = imag[i + j];
                let v_real = real[i + j + len / 2] * w_real - imag[i + j + len / 2] * w_imag;
                let v_imag = real[i + j + len / 2] * w_imag + imag[i + j + len / 2] * w_real;
                real[i + j] = u_real + v_real;
                imag[i + j] = u_imag + v_imag;
                real[i + j + len / 2] = u_real - v_real;
                imag[i + j + len / 2] = u_imag - v_imag;
            }
        }
    }
}

pub(super) fn ifft(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    for val in imag.iter_mut() {
        *val = -*val;
    }
    fft(real, imag);
    for (r, i) in real.iter_mut().zip(imag.iter_mut()) {
        *r /= n as f32;
        *i /= -(n as f32);
    }
}
