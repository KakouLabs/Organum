use rayon::prelude::*;

use crate::resampler::{
    types::ResampleRequest,
    common::utils,
};

pub fn generate_pitch(
    req: &ResampleRequest,
    vuv_render: &[bool],
    f0_off_render: &[f64],
    target_midi: f64,
    modulation: f64,
    fps: f64,
    render_length: usize,
) -> Vec<f64> {
    let pitchbend_semitones = utils::parse_pitchbend_to_semitones(&req.pitchbend);
    let pps = 8.0 * req.tempo as f64 / 5.0;
    let pitchbend_interp = utils::CubicSplineInterpolator::new(&pitchbend_semitones);

    let f0_map = |i: usize| {
        if vuv_render[i] {
            let t = (i as f64) / fps;
            let pb_idx = t * pps;
            let pb = pitchbend_interp.sample(pb_idx);
            let f0_mod = f0_off_render[i] * modulation;
            utils::midi_to_hz(target_midi + pb + f0_mod)
        } else {
            0.0
        }
    };

    const PAR_THRESHOLD: usize = 2048;
    if render_length < PAR_THRESHOLD {
        (0..render_length).map(f0_map).collect()
    } else {
        (0..render_length).into_par_iter().map(f0_map).collect()
    }
}
