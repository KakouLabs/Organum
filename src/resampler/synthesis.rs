pub fn synthesize(
    f0: &Vec<f64>,
    sp: &mut Vec<Vec<f64>>,
    ap: &mut Vec<Vec<f64>>,
    sample_rate: u32,
    frame_period: f64,
) -> Vec<f64> {
    rsworld::synthesis(f0, sp, ap, frame_period, sample_rate as i32)
}

pub fn warp_spectrum(sp: &mut Vec<f64>, fs: f64, factor: f64) {
    if (factor - 1.0).abs() < 0.001 {
        return;
    }
    let len = sp.len();
    let original = sp.to_owned();
    let df = fs / ((len - 1) as f64 * 2.0);

    for (i, v) in sp.iter_mut().enumerate().take(len) {
        let f_dst = i as f64 * df;
        let m_dst = 2595.0 * (1.0 + f_dst / 700.0).log10();
        let m_src = m_dst * factor;
        let f_src = 700.0 * (10.0f64.powf(m_src / 2595.0) - 1.0);

        let src_idx = f_src / df;

        if src_idx >= (len - 1) as f64 {
            *v = original[len - 1];
        } else {
            let idx_floor = src_idx as usize;
            let t = src_idx - idx_floor as f64;
            *v = original[idx_floor] * (1.0 - t) + original[idx_floor + 1] * t;
        }
    }
}
