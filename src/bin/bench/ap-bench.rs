use organum::resampler::device::DevicePolicy;
use organum::resampler::synthesis::{
    apply_aperiodicity_cpu_batch, gpu_warp_stats, reset_gpu_warp_stats,
    try_apply_aperiodicity_gpu_batch, WarpBackend,
};
use std::time::Instant;

#[derive(Clone, Copy)]
struct BenchCase {
    name: &'static str,
    frames: usize,
    bins: usize,
    warmup: usize,
    iterations: usize,
    repeats: usize,
}

fn make_ap(frames: usize, bins: usize) -> Vec<Vec<f64>> {
    (0..frames)
        .map(|i| {
            (0..bins)
                .map(|b| {
                    let x = (i as f64 * 0.0113) + (b as f64 * 0.0057);
                    (0.45 + 0.35 * x.sin() + 0.20 * x.cos()).clamp(0.0, 1.0)
                })
                .collect::<Vec<f64>>()
        })
        .collect()
}

fn make_vuv(frames: usize) -> Vec<u8> {
    (0..frames).map(|i| u8::from((i % 7) < 5)).collect()
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
    cache_hits: u64,
    cache_misses: u64,
    cache_reallocs: u64,
    buffer_allocations: u64,
    map_errors: u64,
    cache_return_lock_failures: u64,
    chunk_dispatches: u64,
    chosen_threshold: usize,
}

fn run_case(case: BenchCase, backend: WarpBackend) -> Option<BenchResult> {
    if case.repeats == 0 || case.iterations == 0 {
        eprintln!("[{}] skipped (repeats=0 or iterations=0)", case.name);
        return None;
    }

    let threshold = std::env::var("AP_BENCH_GPU_MIN_FRAMES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);

    let chosen = match backend {
        WarpBackend::Cpu => WarpBackend::Cpu,
        WarpBackend::Gpu => {
            let policy = DevicePolicy {
                gpu_warp_enabled: true,
                gpu_warp_min_frames: threshold,
                gpu_ap_min_frames: threshold,
            };
            policy.select_aperiodicity(case.frames).as_warp_backend()
        }
    };

    let original = make_ap(case.frames, case.bins);
    let vuv = make_vuv(case.frames);
    let mut work = original.clone();

    let onset_fadein_frames = 32usize.min(case.frames);
    let h_factor = 0.20;
    let c_factor = 0.25;
    let breathiness_factor = 0.10;
    let b_scale = 0.8;

    if matches!(chosen, WarpBackend::Gpu) {
        reset_gpu_warp_stats();
    }

    for _ in 0..case.warmup {
        for (dst, src) in work.iter_mut().zip(original.iter()) {
            dst.copy_from_slice(src);
        }

        if matches!(chosen, WarpBackend::Gpu) {
            let _ = try_apply_aperiodicity_gpu_batch(
                work.as_mut_slice(),
                &vuv,
                onset_fadein_frames,
                h_factor,
                c_factor,
                breathiness_factor,
                b_scale,
            );
        } else {
            apply_aperiodicity_cpu_batch(
                work.as_mut_slice(),
                &vuv,
                onset_fadein_frames,
                h_factor,
                c_factor,
                breathiness_factor,
                b_scale,
            );
        }
    }

    let mut times = Vec::with_capacity(case.repeats);

    for _ in 0..case.repeats {
        let start = Instant::now();
        for _ in 0..case.iterations {
            for (dst, src) in work.iter_mut().zip(original.iter()) {
                dst.copy_from_slice(src);
            }

            if matches!(chosen, WarpBackend::Gpu) {
                if let Err(e) = try_apply_aperiodicity_gpu_batch(
                    work.as_mut_slice(),
                    &vuv,
                    onset_fadein_frames,
                    h_factor,
                    c_factor,
                    breathiness_factor,
                    b_scale,
                ) {
                    eprintln!(
                        "[{:<11} {:?}] gpu batch failed, fallback to cpu: {}",
                        case.name, chosen, e
                    );
                    apply_aperiodicity_cpu_batch(
                        work.as_mut_slice(),
                        &vuv,
                        onset_fadein_frames,
                        h_factor,
                        c_factor,
                        breathiness_factor,
                        b_scale,
                    );
                }
            } else {
                apply_aperiodicity_cpu_batch(
                    work.as_mut_slice(),
                    &vuv,
                    onset_fadein_frames,
                    h_factor,
                    c_factor,
                    breathiness_factor,
                    b_scale,
                );
            }
        }
        times.push(start.elapsed());
    }

    times.sort_unstable();
    let median_elapsed = times[times.len() / 2];
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

    let stats = if matches!(chosen, WarpBackend::Gpu) {
        gpu_warp_stats()
    } else {
        Default::default()
    };

    if matches!(chosen, WarpBackend::Gpu) {
        println!(
            "             cache hits/misses/reallocs={}/{}/{} allocs={} map_errors={} return_lock_failures={}",
            stats.cache_hits,
            stats.cache_misses,
            stats.cache_reallocs,
            stats.buffer_allocations,
            stats.map_errors,
            stats.cache_return_lock_failures
        );
        println!("             chunk_dispatches={}", stats.chunk_dispatches);
    }

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
        cache_hits: stats.cache_hits,
        cache_misses: stats.cache_misses,
        cache_reallocs: stats.cache_reallocs,
        buffer_allocations: stats.buffer_allocations,
        map_errors: stats.map_errors,
        cache_return_lock_failures: stats.cache_return_lock_failures,
        chunk_dispatches: stats.chunk_dispatches,
        chosen_threshold: threshold,
    })
}

fn main() {
    let cases = [
        BenchCase {
            name: "short",
            frames: 512,
            bins: 1024,
            warmup: 5,
            iterations: 120,
            repeats: 5,
        },
        BenchCase {
            name: "medium",
            frames: 2048,
            bins: 1024,
            warmup: 3,
            iterations: 30,
            repeats: 5,
        },
        BenchCase {
            name: "long",
            frames: 8192,
            bins: 1024,
            warmup: 2,
            iterations: 16,
            repeats: 5,
        },
        BenchCase {
            name: "stress_mix",
            frames: 4096,
            bins: 1024,
            warmup: 2,
            iterations: 20,
            repeats: 10,
        },
    ];

    println!("============================================================");
    println!(" organum aperiodicity micro-benchmark");
    println!(" config: GPU path uses wgpu when built with --features gpu-warp");
    println!(
        " config: AP_BENCH_GPU_MIN_FRAMES={}",
        std::env::var("AP_BENCH_GPU_MIN_FRAMES").unwrap_or_else(|_| "1".to_string())
    );
    println!(
        " config: WARP_GPU_CHUNK_FRAMES={}",
        std::env::var("WARP_GPU_CHUNK_FRAMES").unwrap_or_else(|_| "auto(full-frame)".to_string())
    );
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
        if !matches!(gpu.backend, WarpBackend::Gpu) {
            println!(
                "[{:<11}] GPU route not selected (threshold={}, frames={})",
                cpu.name, gpu.chosen_threshold, gpu.frames
            );
            continue;
        }

        let median_ratio = gpu.median_us_per_frame / cpu.median_us_per_frame;
        let p95_ratio = gpu.p95_us_per_frame / cpu.p95_us_per_frame;
        let eval_str = if median_ratio < 1.0 {
            "FASTER"
        } else {
            "SLOWER"
        };

        println!(
            "[{:<11}] GPU is {:>6.2}% {:<6} than CPU (p95: {:>+6.2}%)",
            cpu.name,
            ((median_ratio - 1.0) * 100.0).abs(),
            eval_str,
            (p95_ratio - 1.0) * 100.0
        );
        println!(
            "CI_SUMMARY,case={},threshold={},median_ratio={:.4},p95_ratio={:.4}",
            cpu.name, gpu.chosen_threshold, median_ratio, p95_ratio
        );
    }

    println!("\n--- Routing Recommendation ---");
    println!("rule: GPU when median improves >=3% and p95 regression <=5%");
    for (cpu, gpu) in results_cpu.iter().zip(results_gpu.iter()) {
        if !matches!(gpu.backend, WarpBackend::Gpu) {
            println!(
                "[{:<11}] recommend=CPU reason=threshold-gated threshold={} frames={}",
                cpu.name, gpu.chosen_threshold, gpu.frames
            );
            continue;
        }

        let median_ratio = gpu.median_us_per_frame / cpu.median_us_per_frame;
        let p95_ratio = gpu.p95_us_per_frame / cpu.p95_us_per_frame;
        let gpu_ok = median_ratio <= 0.97 && p95_ratio <= 1.05;
        let recommended = if gpu_ok { "GPU" } else { "CPU" };

        println!(
            "[{:<11}] recommend={} median_ratio={:.4} p95_ratio={:.4}",
            cpu.name, recommended, median_ratio, p95_ratio
        );
    }

    println!("\n--- CSV Output ---");
    println!("name,backend,frames,bins,iters,repeats,gpu_min_frames,median_secs,p95_secs,median_us_frame,p95_us_frame,mbins_sec,cache_hits,cache_misses,cache_reallocs,buffer_allocations,map_errors,cache_return_lock_failures,chunk_dispatches");
    for r in results_cpu.iter().chain(results_gpu.iter()) {
        println!(
            "{},{:?},{},{},{},{},{},{:.4},{:.4},{:.2},{:.2},{:.2},{},{},{},{},{},{},{}",
            r.name,
            r.backend,
            r.frames,
            r.bins,
            r.iterations,
            r.repeats,
            r.chosen_threshold,
            r.median_secs,
            r.p95_secs,
            r.median_us_per_frame,
            r.p95_us_per_frame,
            r.mbin_per_sec,
            r.cache_hits,
            r.cache_misses,
            r.cache_reallocs,
            r.buffer_allocations,
            r.map_errors,
            r.cache_return_lock_failures,
            r.chunk_dispatches
        );
    }
}
