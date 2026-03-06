use organum::resampler::synthesis::{
    try_apply_warp_batch_with_backend, WarpBackend, WarpDispatchConfig, WarpLut,
};
use std::time::Instant;

#[derive(Clone, Copy)]
struct BenchCase {
    name: &'static str,
    frames: usize,
    bins: usize,
    factor: f64,
    iterations: usize,
}

fn make_spectrum(frames: usize, bins: usize) -> Vec<Vec<f64>> {
    (0..frames)
        .map(|i| {
            (0..bins)
                .map(|b| {
                    let x = (i as f64 * 0.017) + (b as f64 * 0.0031);
                    (x.sin().abs() + 1e-6) * (1.0 + (b as f64 / bins as f64) * 0.2)
                })
                .collect::<Vec<f64>>()
        })
        .collect()
}

fn run_case(case: BenchCase, backend: WarpBackend) {
    let dispatch = WarpDispatchConfig {
        gpu_warp_enabled: matches!(backend, WarpBackend::Gpu),
        gpu_warp_min_frames: 1,
    };
    let chosen = dispatch.choose_backend(case.frames);

    let lut = WarpLut::new(case.bins, 44_100.0, case.factor);
    let original = make_spectrum(case.frames, case.bins);
    let mut work = original.clone();

    let start = Instant::now();
    for _ in 0..case.iterations {
        for (dst, src) in work.iter_mut().zip(original.iter()) {
            dst.copy_from_slice(src);
        }

        if let Err(e) = try_apply_warp_batch_with_backend(work.as_mut_slice(), &lut, chosen) {
            eprintln!(
                "[{} {:?}] gpu batch failed, fallback to cpu: {}",
                case.name, chosen, e
            );
            for frame in work.iter_mut() {
                lut.apply(frame);
            }
        }
    }
    let elapsed = start.elapsed();

    let total_frames = case.frames * case.iterations;
    let total_bins = total_frames * case.bins;
    let secs = elapsed.as_secs_f64();
    let us_per_frame = (secs * 1_000_000.0) / total_frames as f64;
    let mbin_per_sec = (total_bins as f64 / 1_000_000.0) / secs;

    println!(
        "[{:<11}] {:<10} frames={} bins={} iters={} time={:.3}s us/frame={:.2} Mbins/s={:.2}",
        case.name,
        format!("{:?}", chosen),
        case.frames,
        case.bins,
        case.iterations,
        secs,
        us_per_frame,
        mbin_per_sec
    );
}

fn main() {
    let cases = [
        BenchCase {
            name: "short",
            frames: 512,
            bins: 2049,
            factor: 1.10,
            iterations: 120,
        },
        BenchCase {
            name: "long",
            frames: 8192,
            bins: 2049,
            factor: 1.10,
            iterations: 16,
        },
    ];

    println!("warp_spectrum micro-benchmark (GPU path uses wgpu when built with --features gpu-warp)");
    for case in cases {
        run_case(case, WarpBackend::Cpu);
        run_case(case, WarpBackend::Gpu);
    }
}
