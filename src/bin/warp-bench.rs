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
    warmup: usize,
    iterations: usize,
    repeats: usize,
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

struct BenchResult {
    name: &'static str,
    backend: WarpBackend,
    frames: usize,
    bins: usize,
    iterations: usize,
    repeats: usize,
    median_secs: f64,
    p95_secs: f64,
    median_us_per_frame: f64,
    p95_us_per_frame: f64,
    mbin_per_sec: f64,
}

fn run_case(case: BenchCase, backend: WarpBackend) -> Option<BenchResult> {
    if case.repeats == 0 || case.iterations == 0 {
        eprintln!("[{}] skipped (repeats=0 or iterations=0)", case.name);
        return None;
    }

    let dispatch = WarpDispatchConfig {
        gpu_warp_enabled: matches!(backend, WarpBackend::Gpu),
        gpu_warp_min_frames: 1,
    };
    let chosen = dispatch.choose_backend(case.frames);

    let lut = WarpLut::new(case.bins, 44_100.0, case.factor);
    let original = make_spectrum(case.frames, case.bins);
    let mut work = original.clone();

    // Warmup
    for _ in 0..case.warmup {
        for (dst, src) in work.iter_mut().zip(original.iter()) {
            dst.copy_from_slice(src);
        }
        let _ = try_apply_warp_batch_with_backend(work.as_mut_slice(), &lut, chosen);
    }

    let mut times = Vec::with_capacity(case.repeats);

    for _ in 0..case.repeats {
        let start = Instant::now();
        for _ in 0..case.iterations {
            for (dst, src) in work.iter_mut().zip(original.iter()) {
                dst.copy_from_slice(src);
            }

            if let Err(e) = try_apply_warp_batch_with_backend(work.as_mut_slice(), &lut, chosen) {
                eprintln!(
                    "[{:<11} {:?}] gpu batch failed, fallback to cpu: {}",
                    case.name, chosen, e
                );
                for frame in work.iter_mut() {
                    lut.apply(frame);
                }
            }
        }
        times.push(start.elapsed());
    }

    times.sort_unstable();
    let median_elapsed = times[times.len() / 2];
    
    // Calculate P95
    let p95_index = (times.len() as f64 * 0.95).floor() as usize;
    let p95_index = p95_index.min(times.len().saturating_sub(1));
    let p95_elapsed = times[p95_index];

    let total_frames = case.frames * case.iterations;
    let total_bins = total_frames * case.bins;
    let secs = median_elapsed.as_secs_f64();
    let p95_secs = p95_elapsed.as_secs_f64();
    
    let us_per_frame = if total_frames > 0 {
        (secs * 1_000_000.0) / total_frames as f64
    } else {
        0.0
    };
    
    let p95_us_per_frame = if total_frames > 0 {
        (p95_secs * 1_000_000.0) / total_frames as f64
    } else {
        0.0
    };

    let mbin_per_sec = if secs > 0.0 {
        (total_bins as f64 / 1_000_000.0) / secs
    } else {
        0.0
    };

    println!(
        "[{:<11}] {:<10} frames={} bins={} iters={}x{} time={:.3}s (p95: {:.3}s) us/frame={:.2} (p95: {:.2}) Mbins/s={:.2}",
        case.name,
        format!("{:?}", chosen),
        case.frames,
        case.bins,
        case.iterations,
        case.repeats,
        secs,
        p95_secs,
        us_per_frame,
        p95_us_per_frame,
        mbin_per_sec
    );

    Some(BenchResult {
        name: case.name,
        backend: chosen,
        frames: case.frames,
        bins: case.bins,
        iterations: case.iterations,
        repeats: case.repeats,
        median_secs: secs,
        p95_secs,
        median_us_per_frame: us_per_frame,
        p95_us_per_frame,
        mbin_per_sec,
    })
}

fn main() {
    let cases = [
        BenchCase {
            name: "short",
            frames: 512,
            bins: 2049,
            factor: 1.10,
            warmup: 5,
            iterations: 120,
            repeats: 5,
        },
        BenchCase {
            name: "medium",
            frames: 2048,
            bins: 2049,
            factor: 1.10,
            warmup: 3,
            iterations: 30,
            repeats: 5,
        },
        BenchCase {
            name: "long",
            frames: 8192,
            bins: 2049,
            factor: 1.10,
            warmup: 2,
            iterations: 16,
            repeats: 5,
        },
        BenchCase {
            name: "stress_mix",
            frames: 4096,
            bins: 2049,
            factor: 1.10,
            warmup: 2,
            iterations: 20,
            repeats: 10,
        },
    ];

    println!("============================================================");
    println!(" organum warp_spectrum micro-benchmark");
    println!(" config: GPU path uses wgpu when built with --features gpu-warp");
    println!("============================================================\n");
    
    let mut results_cpu = Vec::new();
    let mut results_gpu = Vec::new();

    for &case in &cases {
        if let Some(r) = run_case(case, WarpBackend::Cpu) {
            results_cpu.push(r);
        }
        if let Some(r) = run_case(case, WarpBackend::Gpu) {
            results_gpu.push(r);
        }
    }

    println!("\n--- Summary (GPU / CPU Ratio) ---");
    for (cpu, gpu) in results_cpu.iter().zip(results_gpu.iter()) {
        let median_ratio = gpu.median_us_per_frame / cpu.median_us_per_frame;
        let p95_ratio = gpu.p95_us_per_frame / cpu.p95_us_per_frame;
        
        let median_pct = (median_ratio - 1.0) * 100.0;
        let p95_pct = (p95_ratio - 1.0) * 100.0;

        let eval_str = if median_ratio < 1.0 { "FASTER" } else { "SLOWER" };

        println!(
            "[{:<11}] GPU is {:>6.2}% {:<6} than CPU (p95: {:>+6.2}%)",
            cpu.name,
            median_pct.abs(),
            eval_str,
            p95_pct
        );
    }

    println!("\n--- CSV Output ---");
    println!("name,backend,frames,bins,iters,repeats,median_secs,p95_secs,median_us_frame,p95_us_frame,mbins_sec");
    for r in results_cpu.iter().chain(results_gpu.iter()) {
        println!(
            "{},{:?},{},{},{},{},{:.4},{:.4},{:.2},{:.2},{:.2}",
            r.name,
            r.backend,
            r.frames,
            r.bins,
            r.iterations,
            r.repeats,
            r.median_secs,
            r.p95_secs,
            r.median_us_per_frame,
            r.p95_us_per_frame,
            r.mbin_per_sec
        );
    }
}
