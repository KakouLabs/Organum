use anyhow::Result;
use std::path::Path;
use std::time::Instant;

use crate::resampler::{
    io::audio::{read_audio, write_audio},
    io::features::{generate_features, read_features, write_features},
    common::flags::parse_flags,
    stages::timing::calculate_timing,
    stages::spectrum::apply_warp_and_tilt,
    stages::aperiodicity::apply_aperiodicity_mods,
    stages::pitch::generate_pitch,
    stages::dynamics::apply_dynamics,
    synthesis::synthesize,
    types::ResampleRequest,
    common::utils::{self, interpolate_frames, to_feature_path},
    common::consts,
};

pub fn resample(req: &ResampleRequest) -> Result<()> {
    let start_total = Instant::now();
    let config = crate::config::global_config();
    let sample_rate = config.sample_rate;
    let frame_period = config.frame_period;
    let feat_ext = &config.feature_extension;

    let input_path = Path::new(&req.input_file);
    let output_path = Path::new(&req.output_file);
    let feature_path = to_feature_path(input_path, feat_ext);

    let start_features = Instant::now();
    let features = if feature_path.exists() {
        match read_features(&feature_path) {
            Ok(f) => {
                tracing::info!("cache hit: loaded features from {:?}", feature_path);
                f
            }
            Err(e) => {
                tracing::warn!(
                    "cache miss: cache {:?} is invalid ({}), regenerating",
                    feature_path,
                    e
                );
                let audio = read_audio(input_path, sample_rate)?;
                let features = generate_features(&audio, sample_rate, frame_period)?;
                let _ = write_features(&feature_path, &features, config.zstd_compression_level);
                features
            }
        }
    } else {
        tracing::info!("cache miss: no cache file for {:?}", input_path);
        let audio = read_audio(input_path, sample_rate)?;
        let features = generate_features(&audio, sample_rate, frame_period)?;
        let _ = write_features(&feature_path, &features, config.zstd_compression_level);
        features
    };

    tracing::debug!("Feature stage completed in {:?}", start_features.elapsed());

    let start_synthesis = Instant::now();
    let fps = 1000.0 / frame_period;

    // Build timing map and frame-aligned pitch offsets.
    let timing = calculate_timing(req, &features.f0, features.base_f0, fps)?;

    // Resample feature curves onto the render timeline.
    let mgc_render = interpolate_frames(&features.mgc, &timing.t_render);
    let bap_render = interpolate_frames(&features.bap, &timing.t_render);

    let parsed_flags = parse_flags(&req.flags);

    // Decode WORLD envelopes for synthesis.
    let start_decode = Instant::now();
    let mut sp_render = rsworld::decode_spectral_envelope(
        &mgc_render,
        timing.render_length as i32,
        sample_rate as i32,
        consts::FFT_SIZE,
    );
    let mut ap_render =
        rsworld::decode_aperiodicity(&bap_render, timing.render_length as i32, sample_rate as i32);
    tracing::debug!("Decode stage completed in {:?}", start_decode.elapsed());

    // Resolve pitch/formant parameters from flags.
    let g_factor = if parsed_flags.g != 0.0 {
        2.0_f64.powf(parsed_flags.g / 100.0)
    } else {
        1.0
    };
    let f_factor = if parsed_flags.f != 0.0 {
        2.0_f64.powf(parsed_flags.f / 12.0)
    } else {
        1.0
    };
    let total_factor = g_factor * f_factor;

    let target_midi = utils::note_to_midi(&req.tone) as f64 + (parsed_flags.t / 100.0);
    let target_base_f0 = utils::midi_to_hz(target_midi);

    // Apply spectral warp and high-frequency tilt.
    apply_warp_and_tilt(
        &mut sp_render,
        sample_rate,
        timing.render_length,
        total_factor,
        target_base_f0,
    );

    // Apply voiced/unvoiced aperiodicity shaping.
    apply_aperiodicity_mods(
        &mut ap_render,
        &timing.vuv_render,
        timing.render_length,
        timing.scaled_cons_sec,
        fps,
        parsed_flags.h,
        parsed_flags.c,
        parsed_flags.b,
    );

    // Generate render-time F0 trajectory.
    let modulation = parsed_flags.m / 100.0;
    let f0_render = generate_pitch(
        req,
        &timing.vuv_render,
        &timing.f0_off_render,
        target_midi,
        modulation,
        fps,
        timing.render_length,
    );

    let volume = parsed_flags.a.clamp(0.0, 200.0) / 100.0;

    // Run WORLD synthesis.
    let start_world_synth = Instant::now();
    let mut syn = synthesize(
        &f0_render,
        &mut sp_render,
        &mut ap_render,
        sample_rate,
        frame_period,
    );
    tracing::debug!(
        "WORLD synthesis stage completed in {:?}",
        start_world_synth.elapsed()
    );

    // Apply dynamics and output gain.
    apply_dynamics(&mut syn, parsed_flags.d, parsed_flags.p, volume);

    // Write final waveform.
    let start_write = Instant::now();
    write_audio(output_path, &syn, sample_rate)?;
    tracing::debug!("Write stage completed in {:?}", start_write.elapsed());

    tracing::info!(
        "resample completed for {:?} in {:?} (total {:?})",
        req.input_file,
        start_synthesis.elapsed(),
        start_total.elapsed()
    );
    Ok(())
}
