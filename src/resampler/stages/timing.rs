use anyhow::Result;
use rayon::prelude::*;

use crate::{
    config::QualityPreset,
    resampler::{common::utils::LinearInterpolator, types::ResampleRequest},
};

pub struct TimingData {
    pub render_length: usize,
    pub t_render: Vec<f32>,
    pub f0_off_render: Vec<f32>,
    pub vuv_render: Vec<u8>,
    pub scaled_cons_sec: f32,
}

pub fn calculate_timing(
    req: &ResampleRequest,
    f0: &[f32],
    base_f0: f32,
    fps: f32,
) -> Result<TimingData> {
    calculate_timing_impl(
        req,
        f0.len(),
        f0.iter().copied(),
        base_f0,
        fps,
        QualityPreset::Classic,
    )
}

pub fn calculate_timing_with_quality(
    req: &ResampleRequest,
    f0: &[f32],
    base_f0: f32,
    fps: f32,
    quality_preset: QualityPreset,
) -> Result<TimingData> {
    calculate_timing_impl(
        req,
        f0.len(),
        f0.iter().copied(),
        base_f0,
        fps,
        quality_preset,
    )
}

fn calculate_timing_impl(
    req: &ResampleRequest,
    feature_length: usize,
    f0_values: impl IntoIterator<Item = f32>,
    base_f0: f32,
    fps: f32,
    quality_preset: QualityPreset,
) -> Result<TimingData> {
    if feature_length == 0 {
        return Err(anyhow::anyhow!("No features found in input file"));
    }

    let feature_length_sec = feature_length as f32 / fps;
    let safe_base_f0 = if base_f0.is_finite() && base_f0 > 0.0 {
        base_f0
    } else {
        440.0
    };
    let base_f0_log2 = safe_base_f0.log2();
    let mut vuv = Vec::with_capacity(feature_length);
    let mut f0_off = Vec::with_capacity(feature_length);
    for f in f0_values {
        if f > 0.0 {
            vuv.push(1);
            f0_off.push(12.0 * (f.log2() - base_f0_log2));
        } else {
            vuv.push(0);
            f0_off.push(0.0);
        }
    }
    if vuv.iter().all(|&value| value == 0) {
        vuv.fill(1);
    }

    let start_sec = req.offset / 1000.0;
    let end_sec = if req.cutoff < 0.0 {
        start_sec - req.cutoff / 1000.0
    } else {
        (feature_length_sec - req.cutoff / 1000.0).max(start_sec)
    };

    let cons_sec = req.fixed_length / 1000.0;
    let velocity = if req.velocity <= 0.0 {
        100.0
    } else {
        req.velocity
    };
    let cons_stretch = 2.0_f32.powf(1.0 - velocity / 100.0);

    let mut actual_cons_sec = cons_sec.min(end_sec - start_sec).max(0.0);
    let mut scaled_cons_sec = actual_cons_sec * cons_stretch;

    let length_req_sec = req.length_req / 1000.0;

    if scaled_cons_sec > length_req_sec {
        scaled_cons_sec = length_req_sec;
        actual_cons_sec = scaled_cons_sec / cons_stretch;
    }

    let render_length = (length_req_sec * fps).round() as usize;
    if render_length == 0 {
        return Err(anyhow::anyhow!("Calculated render length is 0"));
    }

    let cons_stretch_recip = 1.0 / cons_stretch;
    let vowel_time_req = (length_req_sec - scaled_cons_sec).max(0.001);
    let vowel_time_src = (end_sec - (start_sec + actual_cons_sec)).max(0.001);
    let vowel_scale = vowel_time_src / vowel_time_req;
    let vowel_base = start_sec + actual_cons_sec;

    let t_map = |i: usize| {
        let t_out_sec = (i as f32) / fps;
        let t_in_sec = if t_out_sec <= scaled_cons_sec && scaled_cons_sec > 0.0 {
            start_sec + (t_out_sec * cons_stretch_recip)
        } else {
            let vowel_time_out = t_out_sec - scaled_cons_sec;
            vowel_base + vowel_time_out * vowel_scale
        };
        t_in_sec * fps
    };

    let t_render: Vec<f32> = if render_length < 1536 {
        (0..render_length).map(t_map).collect()
    } else {
        (0..render_length).into_par_iter().map(t_map).collect()
    };

    let f0_off_interp = LinearInterpolator::new(&f0_off);
    let vuv_map = |&t: &f32| vuv[(t.round() as usize).clamp(0, feature_length - 1)];
    let (f0_off_render, mut vuv_render): (Vec<f32>, Vec<u8>) = if render_length < 1536 {
        t_render
            .iter()
            .map(|&t| (f0_off_interp.sample(t), vuv_map(&t)))
            .unzip()
    } else {
        t_render
            .par_iter()
            .map(|&t| (f0_off_interp.sample(t), vuv_map(&t)))
            .unzip()
    };
    for i in 1..render_length.saturating_sub(1) {
        let sum = vuv_render[i - 1] as u16 + vuv_render[i] as u16 + vuv_render[i + 1] as u16;
        vuv_render[i] = (sum >= 2) as u8;
    }

    stabilize_vuv_runs(&mut vuv_render, render_length, quality_preset);

    Ok(TimingData {
        render_length,
        t_render,
        f0_off_render,
        vuv_render,
        scaled_cons_sec,
    })
}

fn stabilize_vuv_runs(vuv: &mut [u8], render_length: usize, quality_preset: QualityPreset) {
    let min_run = match quality_preset {
        QualityPreset::Classic => return,
        QualityPreset::Balanced => 2,
        QualityPreset::Clear | QualityPreset::BreathySafe => 3,
    };
    if render_length < 3 || vuv.len() < 3 {
        return;
    }

    let short_note = render_length < 24;
    let effective_min_run = if short_note {
        (min_run + 1).min(4)
    } else {
        min_run
    };

    let original = vuv.to_vec();
    let mut start = 0usize;
    while start < original.len() {
        let value = original[start];
        let mut end = start + 1;
        while end < original.len() && original[end] == value {
            end += 1;
        }

        let run_len = end - start;
        if run_len < effective_min_run {
            let left = start.checked_sub(1).map(|i| original[i]);
            let right = original.get(end).copied();
            if let (Some(left), Some(right)) = (left, right) {
                if left == right && left != value {
                    vuv[start..end].fill(left);
                }
            }
        }

        start = end;
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
            pitchbend: None,
        }
    }

    #[test]
    fn all_unvoiced_native_f0_falls_back_to_voiced_render_path() {
        let f0 = vec![0.0; 20];
        let timing =
            calculate_timing(&request(), &f0, 440.0, 200.0).expect("timing should be generated");
        assert!(!timing.vuv_render.is_empty());
        assert!(timing.vuv_render.iter().any(|&value| value != 0));
        assert!(timing.f0_off_render.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn balanced_timing_suppresses_short_vuv_holes() {
        let mut classic = vec![1, 1, 0, 0, 1, 1];
        let mut balanced = classic.clone();

        stabilize_vuv_runs(&mut classic, 6, QualityPreset::Classic);
        stabilize_vuv_runs(&mut balanced, 6, QualityPreset::Balanced);

        assert_eq!(classic, vec![1, 1, 0, 0, 1, 1]);
        assert_eq!(balanced, vec![1, 1, 1, 1, 1, 1]);
    }
}
