use anyhow::Result;
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use organum::cli::init_tracing;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use walkdir::WalkDir;

use organum::resampler::generate_and_cache_features;

#[derive(Parser, Debug)]
#[command(author, version, about = "Organum cache generation tool")]
struct Args {
    path_to_voicebank: String,

    #[arg(short, long)]
    force: bool,

    #[arg(short, long)]
    threads: Option<usize>,

    #[arg(short, long)]
    verbose: bool,

    #[arg(long, default_value = "pretty", value_parser = ["pretty", "json"])]
    log_format: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let json_logs = args.log_format.eq_ignore_ascii_case("json");
    init_tracing(args.verbose, json_logs);

    if let Some(t) = args.threads {
        if t == 0 {
            eprintln!("Error: --threads must be >= 1");
            std::process::exit(1);
        }
        rayon::ThreadPoolBuilder::new()
            .num_threads(t)
            .build_global()?;
    }

    let vb_path = Path::new(&args.path_to_voicebank);
    if !vb_path.exists() || !vb_path.is_dir() {
        eprintln!("Error: Directory does not exist: {:?}", vb_path);
        std::process::exit(1);
    }

    println!("Scanning Voicebank directory: {}", vb_path.display());

    let mut wav_files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(vb_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("wav") {
            wav_files.push(path.to_path_buf());
        }
    }

    if wav_files.is_empty() {
        println!("No .wav files found in the specified directory.");
        return Ok(());
    }

    println!(
        "Found {} .wav files. Starting cache generation...",
        wav_files.len()
    );

    let m = MultiProgress::new();
    let pb = m.add(ProgressBar::new(wav_files.len() as u64));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) - {msg}")
            .map_err(|e| anyhow::anyhow!("invalid progress bar template: {e}"))?
            .progress_chars("=>-"),
    );

    let config = organum::config::load_config();
    let files_to_process: Vec<PathBuf> = wav_files;

    let fail_count = AtomicUsize::new(0);

    files_to_process.par_iter().for_each(|wav_path| {
        let result = if args.force {
            let audio = organum::resampler::read_audio(wav_path, config.sample_rate);
            audio.and_then(|audio| {
                let features = organum::resampler::generate_features(
                    audio,
                    config.sample_rate,
                    config.frame_period,
                )?;
                organum::resampler::write_features(
                    &organum::resampler::to_feature_path(wav_path, &config.feature_extension),
                    &features,
                    config.zstd_compression_level,
                    &config,
                )
            })
        } else {
            generate_and_cache_features(wav_path, &config)
        };

        if let Err(e) = result {
            let file_name = wav_path.file_name().unwrap_or_default().to_string_lossy();
            tracing::warn!("Failed to cache {}: {}", file_name, e);
            fail_count.fetch_add(1, Ordering::Relaxed);
        }

        pb.inc(1);
    });

    let fails = fail_count.load(Ordering::Relaxed);
    if fails > 0 {
        pb.finish_with_message(format!("Done with {} failures.", fails));
        eprintln!("\n{} files failed during caching.", fails);
        std::process::exit(1);
    }

    pb.finish_with_message("Done!");
    println!(
        "\nSuccessfully generated cache (.{} files) for the Voicebank.",
        config.feature_extension
    );

    Ok(())
}
