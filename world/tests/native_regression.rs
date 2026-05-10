#![allow(clippy::all)]

use world::native::{
    code_aperiodicity, code_spectral_envelope, decode_aperiodicity, decode_spectral_envelope,
    AcousticAnalyzer, AcousticConfig, AcousticSynthesizer,
};

const SAMPLE_RATE: i32 = 44_100;
const FRAME_PERIOD: f32 = 5.0;
const FIXTURE_DURATION_SEC: f32 = 0.35;
const CODED_SPECTRAL_DIMS: i32 = 60;

#[derive(Debug)]
struct NativeWorldSnapshot {
    f0: Vec<f32>,
    spectrogram_rows: usize,
    spectrogram_cols: usize,
    spectrogram: Vec<f32>,
    aperiodicity_rows: usize,
    aperiodicity_cols: usize,
    aperiodicity: Vec<f32>,
    coded_spectral_rows: usize,
    coded_spectral_cols: usize,
    coded_spectral: Vec<f32>,
    coded_aperiodicity_rows: usize,
    coded_aperiodicity_cols: usize,
    coded_aperiodicity: Vec<f32>,
    decoded_spectral_rows: usize,
    decoded_spectral_cols: usize,
    decoded_spectral: Vec<f32>,
    decoded_aperiodicity_rows: usize,
    decoded_aperiodicity_cols: usize,
    decoded_aperiodicity: Vec<f32>,
    waveform: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
struct SignalStats {
    mean: f32,
    mean_abs: f32,
    rms: f32,
    min: f32,
    max: f32,
}

fn voiced_vowel_fixture() -> Vec<f32> {
    let sample_count = (SAMPLE_RATE as f32 * FIXTURE_DURATION_SEC) as usize;
    let fundamental = 220.0_f32;
    let second_harmonic = fundamental * 2.0;
    let third_harmonic = fundamental * 3.0;

    (0..sample_count)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let attack = (t / 0.03).min(1.0);
            let release = ((FIXTURE_DURATION_SEC - t) / 0.03).clamp(0.0, 1.0);
            let envelope = attack.min(release);
            let phase = std::f32::consts::TAU * t;

            envelope
                * (0.55 * (phase * fundamental).sin()
                    + 0.24 * (phase * second_harmonic).sin()
                    + 0.11 * (phase * third_harmonic).sin())
        })
        .collect()
}

fn capture_snapshot(audio: &[f32]) -> NativeWorldSnapshot {
    let mut config = AcousticConfig::new(SAMPLE_RATE);
    config.f0_estimation.frame_period = FRAME_PERIOD;

    let mut analyzer = AcousticAnalyzer::with_config(config);
    let features = analyzer.extract_features(audio, SAMPLE_RATE);

    let coded_spectral =
        code_spectral_envelope(&features.spectrogram, SAMPLE_RATE, CODED_SPECTRAL_DIMS);
    let coded_aperiodicity = code_aperiodicity(&features.aperiodicity, SAMPLE_RATE);
    let decoded_spectral =
        decode_spectral_envelope(&coded_spectral, SAMPLE_RATE, features.fft_size);
    let decoded_aperiodicity =
        decode_aperiodicity(&coded_aperiodicity, SAMPLE_RATE, features.fft_size);

    let synthesizer = AcousticSynthesizer::new();
    let waveform = synthesizer.synthesize(
        &features.f0,
        &decoded_spectral,
        &decoded_aperiodicity,
        FRAME_PERIOD,
        SAMPLE_RATE,
    );

    NativeWorldSnapshot {
        f0: features.f0,
        spectrogram_rows: features.spectrogram.rows(),
        spectrogram_cols: features.spectrogram.cols(),
        spectrogram: features.spectrogram.into_vec(),
        aperiodicity_rows: features.aperiodicity.rows(),
        aperiodicity_cols: features.aperiodicity.cols(),
        aperiodicity: features.aperiodicity.into_vec(),
        coded_spectral_rows: coded_spectral.rows(),
        coded_spectral_cols: coded_spectral.cols(),
        coded_spectral: coded_spectral.into_vec(),
        coded_aperiodicity_rows: coded_aperiodicity.rows(),
        coded_aperiodicity_cols: coded_aperiodicity.cols(),
        coded_aperiodicity: coded_aperiodicity.into_vec(),
        decoded_spectral_rows: decoded_spectral.rows(),
        decoded_spectral_cols: decoded_spectral.cols(),
        decoded_spectral: decoded_spectral.into_vec(),
        decoded_aperiodicity_rows: decoded_aperiodicity.rows(),
        decoded_aperiodicity_cols: decoded_aperiodicity.cols(),
        decoded_aperiodicity: decoded_aperiodicity.into_vec(),
        waveform,
    }
}

fn assert_all_finite(label: &str, values: &[f32]) {
    assert!(!values.is_empty(), "{} should not be empty", label);
    assert!(
        values.iter().all(|value| value.is_finite()),
        "{} should contain only finite values",
        label
    );
}

fn assert_close_slice(label: &str, left: &[f32], right: &[f32], tolerance: f32) {
    assert_eq!(left.len(), right.len(), "{} length mismatch", label);

    for (index, (left_value, right_value)) in left.iter().zip(right).enumerate() {
        let delta = (left_value - right_value).abs();
        assert!(
            delta <= tolerance,
            "{}[{}] differs by {}, left={}, right={}",
            label,
            index,
            delta,
            left_value,
            right_value
        );
    }
}

fn signal_stats(values: &[f32]) -> SignalStats {
    assert!(!values.is_empty(), "stats input should not be empty");

    let mut sum = 0.0_f32;
    let mut sum_abs = 0.0_f32;
    let mut sum_squared = 0.0_f32;
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;

    for &value in values {
        sum += value;
        sum_abs += value.abs();
        sum_squared += value * value;
        min = min.min(value);
        max = max.max(value);
    }

    let len = values.len() as f32;
    SignalStats {
        mean: sum / len,
        mean_abs: sum_abs / len,
        rms: (sum_squared / len).sqrt(),
        min,
        max,
    }
}

fn assert_close_value(label: &str, actual: f32, expected: f32, tolerance: f32) {
    let delta = (actual - expected).abs();
    assert!(
        delta <= tolerance,
        "{} differs by {}, actual={}, expected={}",
        label,
        delta,
        actual,
        expected
    );
}

fn assert_stats_close(label: &str, actual: SignalStats, expected: SignalStats, tolerance: f32) {
    assert_close_value(
        &format!("{}.mean", label),
        actual.mean,
        expected.mean,
        tolerance,
    );
    assert_close_value(
        &format!("{}.mean_abs", label),
        actual.mean_abs,
        expected.mean_abs,
        tolerance,
    );
    assert_close_value(
        &format!("{}.rms", label),
        actual.rms,
        expected.rms,
        tolerance,
    );
    assert_close_value(
        &format!("{}.min", label),
        actual.min,
        expected.min,
        tolerance,
    );
    assert_close_value(
        &format!("{}.max", label),
        actual.max,
        expected.max,
        tolerance,
    );
}

#[test]
fn native_world_feature_flow_is_well_formed() {
    let snapshot = capture_snapshot(&voiced_vowel_fixture());
    let frame_count = snapshot.f0.len();

    assert_all_finite("f0", &snapshot.f0);
    assert_all_finite("spectrogram", &snapshot.spectrogram);
    assert_all_finite("aperiodicity", &snapshot.aperiodicity);
    assert_all_finite("coded_spectral", &snapshot.coded_spectral);
    assert_all_finite("coded_aperiodicity", &snapshot.coded_aperiodicity);
    assert_all_finite("decoded_spectral", &snapshot.decoded_spectral);
    assert_all_finite("decoded_aperiodicity", &snapshot.decoded_aperiodicity);
    assert_all_finite("waveform", &snapshot.waveform);

    assert_eq!(snapshot.spectrogram_rows, frame_count);
    assert_eq!(snapshot.aperiodicity_rows, frame_count);
    assert_eq!(snapshot.coded_spectral_rows, frame_count);
    assert_eq!(snapshot.coded_aperiodicity_rows, frame_count);
    assert_eq!(snapshot.decoded_spectral_rows, frame_count);
    assert_eq!(snapshot.decoded_aperiodicity_rows, frame_count);
    assert_eq!(snapshot.coded_spectral_cols, CODED_SPECTRAL_DIMS as usize);
    assert!(snapshot.coded_aperiodicity_cols > 0);
    assert_eq!(snapshot.spectrogram_cols, snapshot.aperiodicity_cols);
    assert_eq!(snapshot.decoded_spectral_cols, snapshot.spectrogram_cols);
    assert_eq!(
        snapshot.decoded_aperiodicity_cols,
        snapshot.aperiodicity_cols
    );

    let voiced_frames = snapshot.f0.iter().filter(|&&f0| f0 > 0.0).count();
    assert!(
        voiced_frames > frame_count / 2,
        "fixture should produce mostly voiced frames"
    );
}

#[test]
fn native_world_feature_flow_is_deterministic() {
    let fixture = voiced_vowel_fixture();
    let first = capture_snapshot(&fixture);
    let second = capture_snapshot(&fixture);

    assert_close_slice("f0", &first.f0, &second.f0, f32::EPSILON);
    assert_close_slice(
        "spectrogram",
        &first.spectrogram,
        &second.spectrogram,
        f32::EPSILON,
    );
    assert_close_slice(
        "aperiodicity",
        &first.aperiodicity,
        &second.aperiodicity,
        f32::EPSILON,
    );
    assert_close_slice(
        "coded_spectral",
        &first.coded_spectral,
        &second.coded_spectral,
        f32::EPSILON,
    );
    assert_close_slice(
        "coded_aperiodicity",
        &first.coded_aperiodicity,
        &second.coded_aperiodicity,
        f32::EPSILON,
    );
    assert_close_slice(
        "decoded_spectral",
        &first.decoded_spectral,
        &second.decoded_spectral,
        f32::EPSILON,
    );
    assert_close_slice(
        "decoded_aperiodicity",
        &first.decoded_aperiodicity,
        &second.decoded_aperiodicity,
        f32::EPSILON,
    );
    assert_close_slice("waveform", &first.waveform, &second.waveform, f32::EPSILON);
}

#[test]
fn native_world_performance_benchmark() {
    let fixture = voiced_vowel_fixture();
    let start = std::time::Instant::now();

    let mut config = AcousticConfig::new(SAMPLE_RATE);
    config.f0_estimation.frame_period = FRAME_PERIOD;

    let mut analyzer = AcousticAnalyzer::with_config(config);
    let features = analyzer.extract_features(&fixture, SAMPLE_RATE);

    let synthesizer = AcousticSynthesizer::new();
    let y = synthesizer.synthesize(
        &features.f0,
        &features.spectrogram,
        &features.aperiodicity,
        FRAME_PERIOD,
        SAMPLE_RATE,
    );

    let duration = start.elapsed();
    println!(
        "Native Analysis + Synthesis of {:.2}s took {:?}",
        FIXTURE_DURATION_SEC, duration
    );
    assert!(y.len() > 0);
}

#[test]
fn native_world_feature_flow_matches_checked_in_signature() {
    let snapshot = capture_snapshot(&voiced_vowel_fixture());
    let tolerance = 1.0e-5;

    assert_stats_close(
        "f0",
        signal_stats(&snapshot.f0),
        SignalStats {
            mean: 216.89885,
            mean_abs: 216.89885,
            rms: 218.45422,
            min: 0.0,
            max: 236.36424,
        },
        tolerance,
    );
    assert_stats_close(
        "spectrogram",
        signal_stats(&snapshot.spectrogram),
        SignalStats {
            mean: 0.24646631,
            mean_abs: 0.24646631,
            rms: 1.7971672,
            min: 4.889239e-14,
            max: 17.847418,
        },
        tolerance,
    );
    assert_stats_close(
        "aperiodicity",
        signal_stats(&snapshot.aperiodicity),
        SignalStats {
            mean: 0.59060574,
            mean_abs: 0.59060574,
            rms: 0.6354526,
            min: 0.001,
            max: 1.0,
        },
        tolerance,
    );
    assert_stats_close(
        "coded_spectral",
        signal_stats(&snapshot.coded_spectral),
        SignalStats {
            mean: -0.07203103,
            mean_abs: 0.8606679,
            rms: 3.0400672,
            min: -21.294268,
            max: 10.086096,
        },
        tolerance,
    );
    assert_stats_close(
        "coded_aperiodicity",
        signal_stats(&snapshot.coded_aperiodicity),
        SignalStats {
            mean: -4.288523,
            mean_abs: 4.288523,
            rms: 4.553499,
            min: -6.07467,
            max: 0.0,
        },
        tolerance,
    );
    assert_stats_close(
        "decoded_spectral",
        signal_stats(&snapshot.decoded_spectral),
        SignalStats {
            mean: 0.24729875,
            mean_abs: 0.24729875,
            rms: 1.8039995,
            min: 1e-12,
            max: 20.803028,
        },
        tolerance,
    );
    assert_stats_close(
        "decoded_aperiodicity",
        signal_stats(&snapshot.decoded_aperiodicity),
        SignalStats {
            mean: 0.59003013,
            mean_abs: 0.59003013,
            rms: 0.6348442,
            min: 0.001,
            max: 1.0,
        },
        tolerance,
    );

    assert_stats_close(
        "waveform",
        signal_stats(&snapshot.waveform),
        SignalStats {
            mean: 0.0006855986,
            mean_abs: 0.35473838,
            rms: 0.43657312,
            min: -0.5454647,
            max: 1.0366029,
        },
        tolerance,
    );
}
