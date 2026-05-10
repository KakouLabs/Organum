use rayon::prelude::*;

use crate::{
    config::QualityPreset,
    resampler::{common::utils, types::ResampleRequest},
};

pub struct PitchArgs<'a> {
    req: &'a ResampleRequest,
    vuv_render: &'a [u8],
    f0_off_render: &'a [f32],
    target_midi: f32,
    modulation: f32,
    fps: f32,
    render_length: usize,
    quality_preset: QualityPreset,
}

#[inline(always)]
fn midi_to_hz_fast(midi: f32) -> f32 {
    const LUT_START: i32 = -48;
    const LUT_END: i32 = 144;
    const LUT_LEN: usize = (LUT_END - LUT_START + 1) as usize;

    static MIDI_HZ_LUT: std::sync::OnceLock<[f32; LUT_LEN]> = std::sync::OnceLock::new();

    let lut = MIDI_HZ_LUT.get_or_init(|| {
        let mut table = [0.0; LUT_LEN];
        let mut i = 0usize;
        while i < LUT_LEN {
            table[i] = utils::midi_to_hz((LUT_START + i as i32) as f32);
            i += 1;
        }
        table
    });

    if midi <= LUT_START as f32 {
        return lut[0];
    }
    if midi >= LUT_END as f32 {
        return lut[LUT_LEN - 1];
    }

    let idx = midi.floor() as i32;
    let frac = midi - idx as f32;
    let base = (idx - LUT_START) as usize;
    lut[base] * (1.0 - frac) + lut[base + 1] * frac
}

pub fn generate_pitch(
    req: &ResampleRequest,
    vuv_render: &[u8],
    f0_off_render: &[f32],
    target_midi: f32,
    modulation: f32,
    fps: f32,
    render_length: usize,
) -> Vec<f32> {
    generate_pitch_with_args(PitchArgs {
        req,
        vuv_render,
        f0_off_render,
        target_midi,
        modulation,
        fps,
        render_length,
        quality_preset: QualityPreset::Classic,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn generate_pitch_with_quality(
    req: &ResampleRequest,
    vuv_render: &[u8],
    f0_off_render: &[f32],
    target_midi: f32,
    modulation: f32,
    fps: f32,
    render_length: usize,
    quality_preset: QualityPreset,
) -> Vec<f32> {
    generate_pitch_with_args(PitchArgs {
        req,
        vuv_render,
        f0_off_render,
        target_midi,
        modulation,
        fps,
        render_length,
        quality_preset,
    })
}

pub fn generate_pitch_with_args(args: PitchArgs<'_>) -> Vec<f32> {
    let pitchbend_semitones = utils::parse_pitchbend_to_semitones(&args.req.pitchbend);
    let pps = 8.0 * args.req.tempo / 5.0;
    let pitchbend_interp = utils::LinearInterpolator::new(&pitchbend_semitones);

    let f0_map = |i: usize| {
        let boundary_f0 = boundary_f0_scale(i, args.vuv_render, args.quality_preset);
        if args.vuv_render[i] != 0 || boundary_f0 > 0.0 {
            let t = (i as f32) / args.fps;
            let pb_idx = t * pps;
            let pb = pitchbend_interp.sample(pb_idx);
            let f0_mod = args.f0_off_render[i] * args.modulation;
            midi_to_hz_fast(args.target_midi + pb + f0_mod) * boundary_f0.max(1.0)
        } else {
            0.0
        }
    };

    const PAR_THRESHOLD: usize = 2048;
    if args.render_length < PAR_THRESHOLD {
        (0..args.render_length).map(f0_map).collect()
    } else {
        (0..args.render_length)
            .into_par_iter()
            .map(f0_map)
            .collect()
    }
}

fn boundary_f0_scale(index: usize, vuv_render: &[u8], quality_preset: QualityPreset) -> f32 {
    if vuv_render.get(index).copied() != Some(0) {
        return 1.0;
    }
    if matches!(quality_preset, QualityPreset::Classic) {
        return 0.0;
    }

    let near_left = index > 0 && vuv_render[index - 1] != 0;
    let near_right = vuv_render.get(index + 1).copied().unwrap_or(0) != 0;
    if near_left || near_right {
        0.001
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ResampleRequest {
        ResampleRequest {
            input_file: "input.wav".to_string(),
            output_file: "output.wav".to_string(),
            tone: "C4".to_string(),
            velocity: 100.0,
            flags: "-".to_string(),
            offset: 0.0,
            length_req: 100.0,
            fixed_length: 0.0,
            cutoff: 0.0,
            tempo: 120.0,
            base_tone: String::new(),
            pitchbend: Some(vec![0, 50, -25, 100]),
        }
    }

    #[test]
    fn balanced_pitch_keeps_boundary_f0_continuous() {
        let vuv = vec![1, 0, 0, 1];
        let f0_off = vec![0.0; vuv.len()];
        let req = request();
        let classic = generate_pitch_with_quality(
            &req,
            &vuv,
            &f0_off,
            60.0,
            0.0,
            200.0,
            vuv.len(),
            QualityPreset::Classic,
        );
        let balanced = generate_pitch_with_quality(
            &req,
            &vuv,
            &f0_off,
            60.0,
            0.0,
            200.0,
            vuv.len(),
            QualityPreset::Balanced,
        );

        assert_eq!(classic[1], 0.0);
        assert!(balanced[1] > 0.0);
        assert!(balanced[2] > 0.0);
    }
}
