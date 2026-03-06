pub fn synthesize(
    f0: &Vec<f64>,
    sp: &mut Vec<Vec<f64>>,
    ap: &mut Vec<Vec<f64>>,
    sample_rate: u32,
    frame_period: f64,
) -> Vec<f64> {
    rsworld::synthesis(f0, sp, ap, frame_period, sample_rate as i32)
}

pub struct WarpLut {
    pub idx_floor: Vec<usize>,
    pub frac: Vec<f64>,
    pub clamped: Vec<bool>,
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
                clamped.push(true);
            } else {
                let fl = src_idx as usize;
                idx_floor.push(fl);
                frac.push(src_idx - fl as f64);
                clamped.push(false);
            }
        }

        Self {
            idx_floor,
            frac,
            clamped,
        }
    }

    #[inline]
    pub fn apply(&self, in_out: &mut Vec<f64>) {
        let original = in_out.clone();
        let last_val = *original.last().unwrap_or(&0.0);

        for (i, v) in in_out.iter_mut().enumerate() {
            if self.clamped[i] {
                *v = last_val;
            } else {
                let fl = self.idx_floor[i];
                let t = self.frac[i];
                *v = original[fl] * (1.0 - t) + original[fl + 1] * t;
            }
        }
    }
}

pub fn warp_spectrum(sp: &mut Vec<f64>, fs: f64, factor: f64) {
    if (factor - 1.0).abs() < 0.001 {
        return;
    }
    let lut = WarpLut::new(sp.len(), fs, factor);
    lut.apply(sp);
}
