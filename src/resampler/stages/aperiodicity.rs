use crate::resampler::{device, device::Device, synthesis};

pub struct AperiodicityStageParams {
    pub scaled_cons_sec: f64,
    pub fps: f64,
    pub h_flag: f64,
    pub c_flag: f64,
    pub b_flag: f64,
    pub device: Device,
}

pub fn apply_aperiodicity_mods(
    ap_render: &mut [Vec<f64>],
    vuv_render: &[bool],
    params: &AperiodicityStageParams,
) {
    let AperiodicityStageParams {
        scaled_cons_sec,
        fps,
        h_flag,
        c_flag,
        b_flag,
        device,
    } = *params;

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

    if matches!(device, Device::Gpu) {
        match synthesis::try_apply_aperiodicity_gpu_batch(
            ap_render,
            vuv_render,
            onset_fadein_frames,
            h_factor,
            c_factor,
            breathiness_factor,
            b_scale,
        ) {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!("GPU aperiodicity failed, falling back to CPU: {}", e);
                device::mark_gpu_unavailable(&format!("aperiodicity stage error: {}", e));
            }
        }
    }

    synthesis::apply_aperiodicity_cpu_batch(
        ap_render,
        vuv_render,
        onset_fadein_frames,
        h_factor,
        c_factor,
        breathiness_factor,
        b_scale,
    );
}
