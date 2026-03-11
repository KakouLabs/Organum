use rayon::prelude::*;

use crate::resampler::{common::utils, types::ResampleRequest};

#[inline(always)]
fn midi_to_hz_fast(midi: f64) -> f64 {
    const LUT_START: i32 = -48;
    const LUT_END: i32 = 144;
    const LUT_LEN: usize = (LUT_END - LUT_START + 1) as usize;

    static MIDI_HZ_LUT: std::sync::OnceLock<[f64; LUT_LEN]> = std::sync::OnceLock::new();

    let lut = MIDI_HZ_LUT.get_or_init(|| {
        let mut table = [0.0; LUT_LEN];
        let mut i = 0usize;
        while i < LUT_LEN {
            table[i] = utils::midi_to_hz((LUT_START + i as i32) as f64);
            i += 1;
        }
        table
    });

    if midi <= LUT_START as f64 {
        return lut[0];
    }
    if midi >= LUT_END as f64 {
        return lut[LUT_LEN - 1];
    }

    let idx = midi.floor() as i32;
    let frac = midi - idx as f64;
    let base = (idx - LUT_START) as usize;
    lut[base] * (1.0 - frac) + lut[base + 1] * frac
}

pub fn generate_pitch(
    req: &ResampleRequest,
    vuv_render: &[u8],
    f0_off_render: &[f64],
    target_midi: f64,
    modulation: f64,
    fps: f64,
    render_length: usize,
) -> Vec<f64> {
    let pitchbend_semitones = utils::parse_pitchbend_to_semitones(&req.pitchbend);
    let pps = 8.0 * req.tempo as f64 / 5.0;
    let pitchbend_interp = utils::LinearInterpolator::new(&pitchbend_semitones);

    let f0_map = |i: usize| {
        if vuv_render[i] != 0 {
            let t = (i as f64) / fps;
            let pb_idx = t * pps;
            let pb = pitchbend_interp.sample(pb_idx);
            let f0_mod = f0_off_render[i] * modulation;
            midi_to_hz_fast(target_midi + pb + f0_mod)
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
