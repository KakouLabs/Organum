use organum::resampler::generate_features;
use organum::resampler::types::{MatrixF64, WorldFeatures};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const SAMPLE_RATE: u32 = 44_100;
const FRAME_PERIOD: f32 = 5.0;

const WORLD_BACKEND: &str = "organum-native";

#[derive(Clone, Copy)]
struct BenchCase {
    name: &'static str,
    duration_sec: f32,
    fundamental_hz: f32,
    breath_noise: f32,
    warmup: usize,
    iterations: usize,
    repeats: usize,
}

#[derive(Clone, Copy)]
struct SignalStats {
    mean: f64,
    mean_abs: f64,
    rms: f64,
    min: f32,
    max: f32,
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(default)
}

fn make_fixture(case: BenchCase) -> Vec<f32> {
    let sample_count = (SAMPLE_RATE as f32 * case.duration_sec) as usize;
    let second_harmonic = case.fundamental_hz * 2.0;
    let third_harmonic = case.fundamental_hz * 3.0;

    (0..sample_count)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let attack = (t / 0.03).min(1.0);
            let release = ((case.duration_sec - t) / 0.04).clamp(0.0, 1.0);
            let envelope = attack.min(release);
            let phase = std::f32::consts::TAU * t;
            let deterministic_noise = ((i as f32 * 12.9898).sin() * 43_758.547).fract() * 2.0 - 1.0;

            envelope
                * (0.55 * (phase * case.fundamental_hz).sin()
                    + 0.24 * (phase * second_harmonic).sin()
                    + 0.11 * (phase * third_harmonic).sin()
                    + case.breath_noise * deterministic_noise)
        })
        .collect()
}

trait BenchFloat: Copy {
    fn to_f64(self) -> f64;
    fn to_f32(self) -> f32;
    fn hash_bits<H: Hasher>(self, hasher: &mut H);
}

impl BenchFloat for f32 {
    fn to_f64(self) -> f64 {
        f64::from(self)
    }

    fn to_f32(self) -> f32 {
        self
    }

    fn hash_bits<H: Hasher>(self, hasher: &mut H) {
        self.to_bits().hash(hasher);
    }
}

impl BenchFloat for f64 {
    fn to_f64(self) -> f64 {
        self
    }

    fn to_f32(self) -> f32 {
        self as f32
    }

    fn hash_bits<H: Hasher>(self, hasher: &mut H) {
        self.to_bits().hash(hasher);
    }
}

fn signal_stats<T: BenchFloat>(values: &[T]) -> SignalStats {
    let mut sum = 0.0;
    let mut sum_abs = 0.0;
    let mut sum_squared = 0.0;
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;

    for &value in values {
        let value_f64 = value.to_f64();
        sum += value_f64;
        sum_abs += value_f64.abs();
        sum_squared += value_f64 * value_f64;
        min = min.min(value.to_f32());
        max = max.max(value.to_f32());
    }

    let len = values.len().max(1) as f64;
    SignalStats {
        mean: sum / len,
        mean_abs: sum_abs / len,
        rms: (sum_squared / len).sqrt(),
        min,
        max,
    }
}

fn hash_float_slice<T: BenchFloat>(values: &[T]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for value in values {
        value.hash_bits(&mut hasher);
    }
    hasher.finish()
}

fn matrix_hash(matrix: &MatrixF64) -> u64 {
    let mut hasher = DefaultHasher::new();
    matrix.rows.hash(&mut hasher);
    matrix.cols.hash(&mut hasher);
    for value in &matrix.data {
        value.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn median_and_p95(times: &mut [Duration]) -> (Duration, Duration) {
    times.sort_unstable();
    let median = times[times.len() / 2];
    let p95_index = ((times.len() as f64 * 0.95).floor() as usize).min(times.len() - 1);
    (median, times[p95_index])
}

fn run_case(audio: &[f32]) -> anyhow::Result<WorldFeatures> {
    generate_features(audio.to_vec(), SAMPLE_RATE, FRAME_PERIOD)
}

fn bench_case(case: BenchCase) -> anyhow::Result<()> {
    if case.iterations == 0 || case.repeats == 0 {
        anyhow::bail!(
            "case {} requires WORLD_BENCH_ITERS and WORLD_BENCH_REPEATS to be non-zero",
            case.name
        );
    }

    let audio = make_fixture(case);

    for _ in 0..case.warmup {
        let _ = run_case(&audio)?;
    }

    let mut times = Vec::with_capacity(case.repeats);
    let mut last_features = None;

    for _ in 0..case.repeats {
        let start = Instant::now();
        let mut features = None;
        for _ in 0..case.iterations {
            features = Some(run_case(&audio)?);
        }
        times.push(start.elapsed());
        last_features = features;
    }

    let features = last_features.expect("iterations should be non-zero");
    let (median, p95) = median_and_p95(&mut times);
    let total_audio_sec = case.duration_sec as f64 * case.iterations as f64;
    let median_rt_factor = median.as_secs_f64() / total_audio_sec;
    let p95_rt_factor = p95.as_secs_f64() / total_audio_sec;
    let f0_stats = signal_stats(&features.f0);
    let mgc_stats = signal_stats(&features.mgc.data);
    let bap_stats = signal_stats(&features.bap.data);

    println!(
        concat!(
            "WORLD_BENCH,backend={},case={},sample_rate={},frame_period={},duration_sec={:.3},",
            "iters={},repeats={},median_ms={:.3},p95_ms={:.3},median_rt_factor={:.4},p95_rt_factor={:.4},",
            "base_f0={:.6},f0_len={},mgc_rows={},mgc_cols={},bap_rows={},bap_cols={},",
            "f0_hash={:016x},mgc_hash={:016x},bap_hash={:016x},",
            "f0_mean={:.9},f0_mean_abs={:.9},f0_rms={:.9},mgc_mean={:.9},mgc_mean_abs={:.9},mgc_rms={:.9},bap_mean={:.9},bap_mean_abs={:.9},bap_rms={:.9},",
            "f0_min={:.9},f0_max={:.9},mgc_min={:.9},mgc_max={:.9},bap_min={:.9},bap_max={:.9}"
        ),
        WORLD_BACKEND,
        case.name,
        SAMPLE_RATE,
        FRAME_PERIOD,
        case.duration_sec,
        case.iterations,
        case.repeats,
        median.as_secs_f64() * 1000.0,
        p95.as_secs_f64() * 1000.0,
        median_rt_factor,
        p95_rt_factor,
        features.base_f0,
        features.f0.len(),
        features.mgc.rows,
        features.mgc.cols,
        features.bap.rows,
        features.bap.cols,
        hash_float_slice(&features.f0),
        matrix_hash(&features.mgc),
        matrix_hash(&features.bap),
        f0_stats.mean,
        f0_stats.mean_abs,
        f0_stats.rms,
        mgc_stats.mean,
        mgc_stats.mean_abs,
        mgc_stats.rms,
        bap_stats.mean,
        bap_stats.mean_abs,
        bap_stats.rms,
        f0_stats.min,
        f0_stats.max,
        mgc_stats.min,
        mgc_stats.max,
        bap_stats.min,
        bap_stats.max,
    );

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let repeats = env_usize("WORLD_BENCH_REPEATS", 5);
    let iterations = env_usize("WORLD_BENCH_ITERS", 3);
    let warmup = env_usize("WORLD_BENCH_WARMUP", 1);
    let cases = [
        BenchCase {
            name: "short_voiced",
            duration_sec: 0.35,
            fundamental_hz: 220.0,
            breath_noise: 0.0,
            warmup,
            iterations,
            repeats,
        },
        BenchCase {
            name: "breathy_mid",
            duration_sec: 0.50,
            fundamental_hz: 196.0,
            breath_noise: 0.035,
            warmup,
            iterations,
            repeats,
        },
        BenchCase {
            name: "high_tense",
            duration_sec: 0.50,
            fundamental_hz: 440.0,
            breath_noise: 0.015,
            warmup,
            iterations,
            repeats,
        },
    ];

    println!("============================================================");
    println!(" organum WORLD analysis benchmark");
    println!(" backend: {}", WORLD_BACKEND);
    println!(" config: WORLD_BENCH_WARMUP={warmup}");
    println!(" config: WORLD_BENCH_ITERS={iterations}");
    println!(" config: WORLD_BENCH_REPEATS={repeats}");
    println!("============================================================");

    for case in cases {
        bench_case(case)?;
    }

    Ok(())
}
