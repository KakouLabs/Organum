use anyhow::Result;
use rayon::join;
use std::path::Path;
use std::time::Instant;

use crate::resampler::{
    common::consts,
    common::flags::parse_flags,
    common::utils::{self, interpolate_frames_matrix_f32, to_feature_path},
    device::DevicePolicy,
    io::audio::write_audio,
    io::cache::load_features_cached,
    stages::aperiodicity::AperiodicityStageParams,
    stages::dynamics::apply_dynamics,
    types::ResampleRequest,
};

use crate::resampler::stages::{
    pitch::generate_pitch_with_quality, timing::calculate_timing_with_quality,
};

use crate::resampler::{
    stages::aperiodicity::apply_aperiodicity_mods, stages::spectrum::apply_warp_and_tilt,
    synthesis::synthesize,
};

struct RenderParams {
    total_factor: f32,
    target_midi: f32,
    target_base_f0: f32,
    warp_device: crate::resampler::device::Device,
    ap_device: crate::resampler::device::Device,
    modulation: f32,
    volume: f32,
}

fn decode_spectral_envelope_for_synthesis(
    mgc_render: world::native::MatrixF32,
    _render_length: usize,
    sample_rate: u32,
) -> world::native::MatrixF32 {
    world::native::decode_spectral_envelope(&mgc_render, sample_rate as i32, consts::FFT_SIZE)
}

fn decode_aperiodicity_for_synthesis(
    bap_render: world::native::MatrixF32,
    _render_length: usize,
    sample_rate: u32,
) -> world::native::MatrixF32 {
    world::native::decode_aperiodicity(&bap_render, sample_rate as i32, consts::FFT_SIZE)
}

fn prepare_render_params(
    req: &ResampleRequest,
    parsed_flags: &crate::resampler::common::flags::ParsedFlags,
    config: &crate::config::OrganumConfig,
    render_length: usize,
) -> RenderParams {
    let g_factor = if parsed_flags.g != 0.0 {
        2.0_f32.powf(parsed_flags.g / 100.0)
    } else {
        1.0
    };
    let f_factor = if parsed_flags.f != 0.0 {
        2.0_f32.powf(parsed_flags.f / 12.0)
    } else {
        1.0
    };
    let total_factor = g_factor * f_factor;

    let target_midi = utils::note_to_midi(&req.tone) as f32 + (parsed_flags.t / 100.0);
    let target_base_f0 = utils::midi_to_hz(target_midi);

    let device_policy = DevicePolicy::from_config(config);
    let warp_device = device_policy.select_warp(render_length);
    let ap_device = device_policy.select_aperiodicity(render_length);
    let modulation = parsed_flags.m / 100.0;
    let volume = parsed_flags.a.clamp(0.0, 200.0) / 100.0;

    RenderParams {
        total_factor,
        target_midi,
        target_base_f0,
        warp_device,
        ap_device,
        modulation,
        volume,
    }
}

pub fn resample(req: &ResampleRequest) -> Result<()> {
    let start_total = Instant::now();
    let config = crate::config::global_config();
    let sample_rate = config.sample_rate;
    let frame_period = config.frame_period;
    let feat_ext = &config.feature_extension;

    let input_path = Path::new(&req.input_file);
    let output_path = Path::new(&req.output_file);
    let feature_path = to_feature_path(input_path, feat_ext);
    let output_dither = config.output_dither;

    let start_features = Instant::now();
    let features_owned = load_features_cached(input_path, &feature_path, config)?;

    tracing::debug!("Feature stage completed in {:?}", start_features.elapsed());

    let start_synthesis = Instant::now();
    let fps = 1000.0 / frame_period;

    let timing = {
        let f0_for_timing: Vec<f32> = features_owned
            .f0
            .iter()
            .map(|&value| value as f32)
            .collect();
        calculate_timing_with_quality(
            req,
            &f0_for_timing,
            features_owned.base_f0 as f32,
            fps,
            config.quality_preset,
        )?
    };

    // Resample feature curves onto the render timeline.
    let (mgc_render, bap_render) = join(
        || interpolate_frames_matrix_f32(&features_owned.mgc, &timing.t_render),
        || interpolate_frames_matrix_f32(&features_owned.bap, &timing.t_render),
    );

    let parsed_flags = parse_flags(&req.flags);

    // Decode WORLD envelopes for synthesis.
    let start_decode = Instant::now();
    let (mut sp_render, mut ap_render) = join(
        || decode_spectral_envelope_for_synthesis(mgc_render, timing.render_length, sample_rate),
        || decode_aperiodicity_for_synthesis(bap_render, timing.render_length, sample_rate),
    );
    tracing::debug!("Decode stage completed in {:?}", start_decode.elapsed());

    // Resolve pitch/formant parameters from flags.
    let render_params = prepare_render_params(req, &parsed_flags, config, timing.render_length);
    let (((), ()), f0_render) = join(
        || {
            join(
                || {
                    apply_warp_and_tilt(
                        &mut sp_render,
                        sample_rate,
                        timing.render_length,
                        render_params.total_factor,
                        render_params.target_base_f0,
                        render_params.warp_device,
                        config.quality_preset,
                    )
                },
                || {
                    apply_aperiodicity_mods(
                        &mut ap_render,
                        &timing.vuv_render,
                        &AperiodicityStageParams {
                            scaled_cons_sec: timing.scaled_cons_sec,
                            fps,
                            h_flag: parsed_flags.h as f32,
                            c_flag: parsed_flags.c as f32,
                            b_flag: parsed_flags.b as f32,
                            device: render_params.ap_device,
                            quality_preset: config.quality_preset,
                        },
                    )
                },
            )
        },
        || {
            generate_pitch_with_quality(
                req,
                &timing.vuv_render,
                &timing.f0_off_render,
                render_params.target_midi,
                render_params.modulation,
                fps,
                timing.render_length,
                config.quality_preset,
            )
        },
    );

    // Run WORLD synthesis.
    let start_world_synth = Instant::now();
    let mut syn = synthesize(
        &f0_render,
        &sp_render,
        &ap_render,
        sample_rate,
        frame_period,
    );
    tracing::debug!(
        "WORLD synthesis stage completed in {:?}",
        start_world_synth.elapsed()
    );

    // Apply dynamics and output gain.
    apply_dynamics(
        &mut syn,
        parsed_flags.d as f32,
        parsed_flags.p as f32,
        render_params.volume,
    );

    // Write final waveform.
    let start_write = Instant::now();
    write_audio(output_path, &syn, sample_rate, output_dither)?;
    tracing::debug!("Write stage completed in {:?}", start_write.elapsed());

    tracing::info!(
        "resample completed for {:?} in {:?} (total {:?})",
        req.input_file,
        start_synthesis.elapsed(),
        start_total.elapsed()
    );
    Ok(())
}
