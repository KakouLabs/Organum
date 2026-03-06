use rayon::prelude::*;

pub fn apply_aperiodicity_mods(
    ap_render: &mut [Vec<f64>],
    vuv_render: &[bool],
    render_length: usize,
    scaled_cons_sec: f64,
    fps: f64,
    h_flag: f64,
    c_flag: f64,
    b_flag: f64,
) {
    let h_factor = if h_flag > 0.0 {
        (h_flag.clamp(0.0, 100.0) / 100.0).powi(2)
    } else {
        0.0
    };
    let c_factor = if c_flag > 0.0 {
        c_flag.clamp(0.0, 100.0) / 100.0
    } else {
        0.0
    };
    let breathiness_factor = (b_flag.clamp(0.0, 100.0) - 50.0) / 50.0;
    let b_scale = b_flag.clamp(0.0, 100.0) / 50.0;

    let onset_fadein_frames = if scaled_cons_sec > 0.0 {
        ((0.050_f64).min(scaled_cons_sec * 0.25) * fps).round() as usize
    } else {
        0
    };

    let apply_ap = |(i, frame): (usize, &mut Vec<f64>)| {
        let is_voiced = vuv_render.get(i).copied().unwrap_or(false);

        let onset_breath_factor = if i < onset_fadein_frames {
            let progress = i as f64 / onset_fadein_frames as f64;
            1.0 - (1.0 - (progress * std::f64::consts::PI).cos()) * 0.5
        } else {
            0.0
        };

        for a in frame.iter_mut() {
            if is_voiced {
                if h_factor > 0.0 {
                    *a *= 1.0 - h_factor;
                }
            } else if c_factor > 0.0 {
                *a *= 1.0 - c_factor;
            }

            if breathiness_factor > 0.0 {
                *a += (1.0 - *a) * breathiness_factor;
            } else if breathiness_factor < 0.0 {
                *a *= b_scale;
            }

            if onset_breath_factor > 0.0 {
                *a += (1.0 - *a) * onset_breath_factor;
            }

            *a = a.clamp(0.0, 1.0);
        }
    };

    const PAR_THRESHOLD: usize = 2048;

    if render_length < PAR_THRESHOLD {
        ap_render.iter_mut().enumerate().for_each(apply_ap);
    } else {
        ap_render.par_iter_mut().enumerate().for_each(apply_ap);
    }
}
