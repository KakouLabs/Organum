use anyhow::Result;
use rayon::prelude::*;

use crate::resampler::{
    types::ResampleRequest,
    common::utils::LinearInterpolator,
};

pub struct TimingData {
    pub render_length: usize,
    pub t_render: Vec<f64>,
    pub f0_off_render: Vec<f64>,
    pub vuv_render: Vec<bool>,
    pub scaled_cons_sec: f64,
}

pub fn calculate_timing(
    req: &ResampleRequest,
    f0: &[f64],
    base_f0: f64,
    fps: f64,
) -> Result<TimingData> {
    let feature_length = f0.len();
    if feature_length == 0 {
        return Err(anyhow::anyhow!("No features found in input file"));
    }

    let feature_length_sec = feature_length as f64 / fps;
    let vuv: Vec<bool> = f0.iter().map(|&f| f > 0.0).collect();
    let base_f0_log2 = base_f0.log2();
    let f0_off: Vec<f64> = f0
        .iter()
        .map(|&f| {
            if f == 0.0 {
                0.0
            } else {
                12.0 * (f.log2() - base_f0_log2)
            }
        })
        .collect();

    let start_sec = req.offset as f64 / 1000.0;
    let end_sec = if req.cutoff < 0.0 {
        start_sec - req.cutoff as f64 / 1000.0
    } else {
        (feature_length_sec - req.cutoff as f64 / 1000.0).max(start_sec)
    };

    let cons_sec = req.fixed_length as f64 / 1000.0;
    let velocity = if req.velocity <= 0.0 {
        100.0
    } else {
        req.velocity as f64
    };
    let cons_stretch = 2.0_f64.powf(1.0 - velocity / 100.0);

    let mut actual_cons_sec = cons_sec.min(end_sec - start_sec).max(0.0);
    let mut scaled_cons_sec = actual_cons_sec * cons_stretch;

    let length_req_sec = req.length_req as f64 / 1000.0;

    if scaled_cons_sec > length_req_sec {
        scaled_cons_sec = length_req_sec;
        actual_cons_sec = scaled_cons_sec / cons_stretch;
    }

    let render_length = (length_req_sec * fps).round() as usize;
    if render_length == 0 {
        return Err(anyhow::anyhow!("Calculated render length is 0"));
    }

    let t_map = |i: usize| {
        let t_out_sec = (i as f64) / fps;
        let t_in_sec = if t_out_sec <= scaled_cons_sec && scaled_cons_sec > 0.0 {
            start_sec + (t_out_sec / cons_stretch)
        } else {
            let vowel_time_out = t_out_sec - scaled_cons_sec;
            let vowel_time_req = (length_req_sec - scaled_cons_sec).max(0.001);
            let vowel_time_src = (end_sec - (start_sec + actual_cons_sec)).max(0.001);
            (start_sec + actual_cons_sec) + vowel_time_out * (vowel_time_src / vowel_time_req)
        };
        t_in_sec * fps
    };

    let t_render: Vec<f64> = if render_length < 2048 {
        (0..render_length).map(t_map).collect()
    } else {
        (0..render_length).into_par_iter().map(t_map).collect()
    };

    let f0_off_interp = LinearInterpolator::new(&f0_off);
    let f0_off_render = f0_off_interp.sample_vec_adaptive(&t_render);
    let vuv_map = |&t: &f64| vuv[(t.round() as usize).clamp(0, feature_length - 1)];
    let vuv_render: Vec<bool> = if render_length < 2048 {
        t_render.iter().map(vuv_map).collect()
    } else {
        t_render.par_iter().map(vuv_map).collect()
    };

    Ok(TimingData {
        render_length,
        t_render,
        f0_off_render,
        vuv_render,
        scaled_cons_sec,
    })
}
