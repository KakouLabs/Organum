use rayon::prelude::*;

pub fn apply_dynamics(
    syn: &mut [f64],
    d_flag: f64,
    p_flag: f64,
    volume: f64,
) {
    const PAR_THRESHOLD: usize = 2048;

    let max_amp_orig = if syn.len() < PAR_THRESHOLD {
        syn.iter().fold(0.0_f64, |acc, &x| acc.max(x.abs()))
    } else {
        syn.par_iter().map(|&x| x.abs()).reduce(|| 0.0_f64, f64::max)
    };

    if max_amp_orig > 0.0 {
        let d_factor = d_flag.clamp(0.0, 100.0) / 100.0;
        let d_enabled = d_factor > 0.0;
        let threshold = 1.0 - d_factor * 0.8;
        let ratio = 1.0 + d_factor * 3.0;

        let peak_after_d = if d_enabled {
            max_amp_orig * (threshold + (1.0 - threshold) / ratio)
        } else {
            max_amp_orig
        };

        let peak_after_vol = peak_after_d * volume;
        let target_peak = if p_flag > 0.0 && p_flag < 100.0 {
            p_flag / 100.0
        } else {
            0.99
        };

        let final_scale = if peak_after_vol > target_peak {
            target_peak / peak_after_d
        } else {
            volume
        };

        // If we only have a simple scale of 1.0 and no D-flag, we can skip the loop
        if d_enabled || (final_scale - 1.0).abs() > 0.001 {
            let apply_syn = |s: &mut f64| {
                if d_enabled {
                    let abs_s = s.abs() / max_amp_orig;
                    if abs_s > threshold {
                        let over = abs_s - threshold;
                        *s = s.signum() * (threshold + over / ratio) * max_amp_orig;
                    }
                }
                *s *= final_scale;
            };
            if syn.len() < PAR_THRESHOLD {
                syn.iter_mut().for_each(apply_syn);
            } else {
                syn.par_iter_mut().for_each(apply_syn);
            }
        }
    }
}
