use super::codec::*;
use super::fft::{fft, ifft};
use super::*;
use std::f32::consts::PI;

#[test]
fn test_blackman_logic() {
    // Test internal Blackman logic used in D4C
    let n = 100;
    for i in 0..n {
        let pos = (i as f32 - n as f32 / 2.0) / (n as f32 / 2.0);
        if pos.abs() < 1.0 {
            let x_win = PI * (pos + 1.0);
            let window = 0.42 - 0.5 * x_win.cos() + 0.08 * (2.0 * x_win).cos();
            assert!(window >= -1e-5 && window <= 1.0 + 1e-5);
        }
    }
}

#[test]
fn test_aperiodicity_db_conversion() {
    let linear = vec![0.1_f32, 0.5, 1.0, 0.0];
    let mut db = vec![0.0; 4];
    let mut back = vec![0.0; 4];

    for i in 0..4 {
        db[i] = 20.0 * linear[i].max(SAFE_GUARD_MINIMUM).log10();
        back[i] = 10.0_f32.powf(db[i] / 20.0);
        assert!((linear[i] - back[i]).abs() < 1e-5);
    }
}

#[test]
fn test_interpolate_axis_edge_cases() {
    let axis = vec![0.0, 10.0, 20.0];
    let values = vec![1.0, 2.0, 3.0];

    assert_eq!(interpolate_axis(&axis, &values, -5.0), 1.0);
    assert_eq!(interpolate_axis(&axis, &values, 25.0), 3.0);
    assert_eq!(interpolate_axis(&axis, &values, 0.0), 1.0);
    assert_eq!(interpolate_axis(&axis, &values, 10.0), 2.0);
    assert!((interpolate_axis(&axis, &values, 5.0) - 1.5).abs() < 1e-5);
}

#[test]
fn interpolation_indices_match_axis_boundary_semantics() {
    let axis = vec![0.0, 10.0, 20.0];
    let targets = vec![-5.0, 0.0, 5.0, 10.0, 15.0, 20.0, 25.0];
    let values = vec![1.0, 2.0, 4.0];
    let indices = interpolation_indices(&axis, &targets);

    for (&target, index) in targets.iter().zip(indices) {
        let indexed = index.interpolate(&values);
        let direct = interpolate_axis(&axis, &values, target);
        assert!(
            (indexed - direct).abs() < 1e-6,
            "target {}: indexed {}, direct {}",
            target,
            indexed,
            direct
        );
    }
}

#[test]
fn test_fft_ifft_roundtrip() {
    let mut real = vec![1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0];
    let mut imag = vec![0.0; 8];
    let original_real = real.clone();

    fft(&mut real, &mut imag);
    ifft(&mut real, &mut imag);

    for (a, b) in real.iter().zip(original_real.iter()) {
        assert!((a - b).abs() < 1e-5);
    }
}

#[test]
fn test_codec_dct_inverse_roundtrip() {
    let input = vec![1.0, 2.0, 3.0, 4.0];
    let mut output = vec![0.0; 4];
    let mut reconstructed = vec![0.0; 4];

    dct_for_codec(&input, &mut output);
    idct_for_codec(&output, &mut reconstructed);

    for (a, b) in input.iter().zip(reconstructed.iter()) {
        assert!((a * input.len() as f32 - b).abs() < 1e-5);
    }
}

#[test]
fn test_mel_conversion_roundtrip() {
    let freq = 1000.0;
    let mel = frequency_to_mel(freq);
    let freq_back = mel_to_frequency(mel);
    assert!((freq - freq_back).abs() < 1e-5);
}

#[test]
fn defaults_match_world_shape_assumptions() {
    let config = SpectralAnalysisConfig::new(44100);
    assert_eq!(config.q1, -0.15);
    assert_eq!(config.f0_floor, 71.0);
    assert_eq!(config.fft_size, 2048);
    assert_eq!(get_number_of_aperiodicities(44100), 5);
}

#[test]
fn dio_silence_matches_existing_smoke_test() {
    let x = vec![0.0; 256];
    let config = F0EstimationConfig::default();
    let (temporal_positions, f0) = dio(&x, 44100, &config);
    assert_eq!(temporal_positions, vec![0.0, 0.005]);
    assert_eq!(f0, vec![0.0, 0.0]);
}

#[test]
fn native_analysis_uses_flat_contiguous_matrices() {
    let x = vec![0.0; 256];
    let fs = 44100;
    let temporal_positions = vec![0.0, 0.005];
    let f0 = vec![0.0, 0.0];
    let mut spectral_config = SpectralAnalysisConfig::new(fs);
    let spectrogram = cheaptrick(&x, fs, &temporal_positions, &f0, &mut spectral_config);
    let aperiodicity = d4c(
        &x,
        fs,
        &temporal_positions,
        &f0,
        spectral_config.fft_size,
        &AperiodicityAnalysisConfig::default(),
    );
    assert_eq!(spectrogram.rows(), f0.len());
    assert_eq!(spectrogram.cols(), 1025);
    assert_eq!(aperiodicity.as_slice()[0], 1.0);
}

#[test]
fn analysis_stages_are_exact_across_repeated_scratch_reuse() {
    let fs = 44100;
    let x: Vec<f32> = (0..2048)
        .map(|i| (2.0 * PI * 220.0 * i as f32 / fs as f32).sin() * 0.25)
        .collect();
    let temporal_positions = vec![0.0, 0.005, 0.010, 0.015];
    let f0 = vec![220.0, 0.0, 330.0, 110.0];
    let mut spectral_config = SpectralAnalysisConfig::new(fs);

    let expected_spectrogram = cheaptrick(&x, fs, &temporal_positions, &f0, &mut spectral_config);
    let expected_aperiodicity = d4c(
        &x,
        fs,
        &temporal_positions,
        &f0,
        spectral_config.fft_size,
        &AperiodicityAnalysisConfig::default(),
    );

    for _ in 0..3 {
        let actual_spectrogram = cheaptrick(&x, fs, &temporal_positions, &f0, &mut spectral_config);
        let actual_aperiodicity = d4c(
            &x,
            fs,
            &temporal_positions,
            &f0,
            spectral_config.fft_size,
            &AperiodicityAnalysisConfig::default(),
        );

        assert_eq!(actual_spectrogram, expected_spectrogram);
        assert_eq!(actual_aperiodicity, expected_aperiodicity);
    }
}

#[test]
fn codec_roundtrip_preserves_shape_without_nan() {
    let spectrogram =
        MatrixF32::from_vec(2, 5, vec![1.0, 2.0, 3.0, 4.0, 5.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let coded = code_spectral_envelope(&spectrogram, 44100, 3);
    let decoded = decode_spectral_envelope(&coded, 44100, 8);
    assert_eq!(coded.rows(), 2);
    assert_eq!(coded.cols(), 3);
    assert_eq!(decoded.rows(), 2);
    assert_eq!(decoded.cols(), 5);
    assert!(decoded.as_slice().iter().all(|value| value.is_finite()));
}

#[test]
fn decode_spectral_envelope_into_handles_size_changes() {
    let coded = MatrixF32::from_vec(2, 4, vec![0.5, -0.1, 0.03, -0.01, 0.4, -0.08, 0.02, -0.005]);
    let mut output = MatrixF32::zeros(0, 0);

    decode_spectral_envelope_into(&coded, 44100, 8, &mut output);
    assert_eq!(output.rows(), 2);
    assert_eq!(output.cols(), 5);
    assert!(output.as_slice().iter().all(|value| value.is_finite()));

    decode_spectral_envelope_into(&coded, 44100, 16, &mut output);
    assert_eq!(output.rows(), 2);
    assert_eq!(output.cols(), 9);
    assert!(output.as_slice().iter().all(|value| value.is_finite()));

    decode_spectral_envelope_into(&coded, 44100, 8, &mut output);
    assert_eq!(output.rows(), 2);
    assert_eq!(output.cols(), 5);
    assert!(output.as_slice().iter().all(|value| value.is_finite()));
}

#[test]
fn synthesis_returns_expected_length() {
    let f0 = vec![0.0, 0.0];
    let spectrogram = MatrixF32::zeros(2, 1025);
    let aperiodicity = MatrixF32::from_vec(2, 1025, vec![1.0; 2 * 1025]);
    let y = synthesis(&f0, &spectrogram, &aperiodicity, 5.0, 44100);
    assert_eq!(y.len(), 441);
    assert!(y.iter().all(|sample| sample.is_finite()));
    assert!(y.iter().any(|sample| *sample != 0.0));
}

#[test]
fn in_place_synthesis_reuses_existing_output_buffer() {
    let f0 = vec![220.0, 220.0];
    let spectrogram = MatrixF32::from_vec(2, 4, vec![1.0; 8]);
    let aperiodicity = MatrixF32::from_vec(2, 4, vec![0.0; 8]);
    let mut y = vec![123.0; 441];
    let capacity = y.capacity();
    synthesis_in_place(&f0, &spectrogram, &aperiodicity, 5.0, 44100, &mut y);
    assert_eq!(y.len(), 441);
    assert_eq!(y.capacity(), capacity);
    assert!(y.iter().all(|sample| sample.is_finite()));
    assert!(y.iter().all(|sample| *sample != 123.0));
}

#[test]
fn synthesis_is_exact_across_repeated_scratch_reuse() {
    let f0 = vec![220.0, 0.0, 246.94, 246.94];
    let spectrogram = MatrixF32::from_vec(
        4,
        4,
        vec![
            1.0, 0.8, 0.5, 0.2, 1.1, 0.7, 0.4, 0.2, 0.9, 0.6, 0.3, 0.1, 1.0, 0.9, 0.7, 0.3,
        ],
    );
    let aperiodicity = MatrixF32::from_vec(
        4,
        4,
        vec![
            0.2, 0.3, 0.6, 0.9, 1.0, 1.0, 1.0, 1.0, 0.3, 0.4, 0.7, 0.9, 0.2, 0.4, 0.6, 0.8,
        ],
    );

    let expected = synthesis(&f0, &spectrogram, &aperiodicity, 5.0, 44100);
    for _ in 0..3 {
        assert_eq!(
            synthesis(&f0, &spectrogram, &aperiodicity, 5.0, 44100),
            expected
        );
    }
}

#[test]
fn synthesis_handles_alternating_shapes_without_stale_scratch() {
    let small_f0 = vec![220.0, 0.0];
    let small_spectrogram = MatrixF32::from_vec(2, 4, vec![1.0, 0.8, 0.5, 0.2, 0.9, 0.7, 0.4, 0.1]);
    let small_aperiodicity =
        MatrixF32::from_vec(2, 4, vec![0.2, 0.3, 0.6, 0.9, 1.0, 1.0, 1.0, 1.0]);
    let large_f0 = vec![330.0, 330.0, 0.0];
    let large_spectrogram = MatrixF32::from_vec(
        3,
        8,
        vec![
            1.0, 0.9, 0.8, 0.7, 0.5, 0.3, 0.2, 0.1, 1.1, 1.0, 0.8, 0.6, 0.5, 0.4, 0.2, 0.1, 0.8,
            0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1,
        ],
    );
    let large_aperiodicity = MatrixF32::from_vec(
        3,
        8,
        vec![
            0.2, 0.2, 0.3, 0.4, 0.6, 0.8, 0.9, 0.95, 0.3, 0.3, 0.4, 0.5, 0.7, 0.85, 0.9, 0.95, 1.0,
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
    );

    let expected_small = synthesis(
        &small_f0,
        &small_spectrogram,
        &small_aperiodicity,
        5.0,
        44100,
    );
    let expected_large = synthesis(
        &large_f0,
        &large_spectrogram,
        &large_aperiodicity,
        5.0,
        44100,
    );

    assert_eq!(
        synthesis(
            &small_f0,
            &small_spectrogram,
            &small_aperiodicity,
            5.0,
            44100
        ),
        expected_small
    );
    assert_eq!(
        synthesis(
            &large_f0,
            &large_spectrogram,
            &large_aperiodicity,
            5.0,
            44100
        ),
        expected_large
    );
    assert_eq!(
        synthesis(
            &small_f0,
            &small_spectrogram,
            &small_aperiodicity,
            5.0,
            44100
        ),
        expected_small
    );
}

#[test]
fn analyze_into_parity_smoke() {
    let x = vec![0.0; 512];
    let fs = 44100;
    let mut config = AcousticConfig::new(fs);
    let mut features = AcousticFeatures::new(config.f0_estimation.frame_period, fs);
    let mut workspace = AnalyzerWorkspace::new();
    analyze_into(&x, fs, &mut config, &mut features, &mut workspace);
    let f0_capacity = features.f0.capacity();
    let spec_capacity = features.spectrogram.as_slice().len();
    analyze_into(&x, fs, &mut config, &mut features, &mut workspace);
    assert_eq!(features.f0.capacity(), f0_capacity);
    assert_eq!(features.spectrogram.as_slice().len(), spec_capacity);
    assert_eq!(features.spectrogram.rows(), features.frame_count());
    assert_eq!(features.spectrogram.cols(), features.bin_count());
    assert_eq!(features.spectrogram.cols(), features.aperiodicity.cols());
}

#[test]
fn borrowed_matrix_view_avoids_copying_cached_flat_features() {
    let config = SpectralAnalysisConfig::new(44100);
    let cols = config.fft_size as usize / 2 + 1;
    let flat = vec![1.0; 2 * cols];
    let view = cheaptrick_from_spectrum_borrowed(&flat, 2, &config);
    assert_eq!(view.rows(), 2);
    assert_eq!(view.cols(), cols);
    assert_eq!(view.as_slice().as_ptr(), flat.as_ptr());
    assert_eq!(view.row(1).len(), cols);
}
