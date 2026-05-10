use crate::resampler::{device, device::Device, synthesis};

pub struct AperiodicityStageParams {
    pub scaled_cons_sec: f32,
    pub fps: f32,
    pub h_flag: f32,
    pub c_flag: f32,
    pub b_flag: f32,
    pub device: Device,
    pub quality_preset: crate::config::QualityPreset,
}

pub fn apply_aperiodicity_mods(
    ap_render: &mut world::native::MatrixF32,
    vuv_render: &[u8],
    params: &AperiodicityStageParams,
) {
    const GPU_AP_MIN_SAFE_FRAMES: usize = 64;

    let AperiodicityStageParams {
        scaled_cons_sec,
        fps,
        h_flag,
        c_flag,
        b_flag,
        device,
        quality_preset,
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
    let raw_breathiness_factor = (b_flag.clamp(0.0, 100.0) - 50.0) / 50.0;
    let spectral_control = h_factor.max(c_factor);
    let breathiness_cap = (1.0 - spectral_control * 0.35).clamp(0.55, 1.0);
    let breathiness_factor = raw_breathiness_factor.clamp(-breathiness_cap, breathiness_cap);
    let b_scale = (1.0 + breathiness_factor).clamp(0.0, 2.0);
    let quality_profile = synthesis::QualityProfile::from_preset(quality_preset);

    let onset_fadein_frames = if scaled_cons_sec > 0.0 {
        ((0.050_f32).min(scaled_cons_sec * 0.25) * fps).round() as usize
    } else {
        0
    };

    let rows = ap_render.rows();
    let cols = ap_render.cols();

    if matches!(device, Device::Gpu) && rows >= GPU_AP_MIN_SAFE_FRAMES {
        match synthesis::try_apply_aperiodicity_gpu_batch(
            ap_render.as_mut_slice(),
            rows,
            cols,
            vuv_render,
            onset_fadein_frames,
            h_factor,
            c_factor,
            breathiness_factor,
            b_scale,
            quality_profile,
        ) {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!("GPU aperiodicity failed, falling back to CPU: {}", e);
                device::mark_gpu_unavailable(&format!("aperiodicity stage error: {}", e));
            }
        }
    }

    synthesis::apply_aperiodicity_cpu_batch(
        ap_render.as_mut_slice(),
        rows,
        cols,
        vuv_render,
        onset_fadein_frames,
        h_factor,
        c_factor,
        breathiness_factor,
        b_scale,
        quality_profile,
    );
}
