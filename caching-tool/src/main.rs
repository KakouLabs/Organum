use anyhow::Result;
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use walkdir::WalkDir;
use tracing_subscriber::EnvFilter;

use organum::resampler::{generate_and_cache_features, is_feature_cache_compatible};

#[derive(Parser, Debug)]
#[command(author, version, about = "Organum cache generation tool", long_about = None)]
struct Args {
    /// Path to the Voicebank directory
    path_to_voicebank: String,

    /// Force regeneration of cache files even if they already exist
    #[arg(short, long)]
    force: bool,

    /// Number of threads to use (default: number of logical cores)
    #[arg(short, long)]
    threads: Option<usize>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Log output format: pretty or json
    #[arg(long, default_value = "pretty", value_parser = ["pretty", "json"])]
    log_format: String,
}

fn init_tracing(verbose: bool, json_logs: bool) {
    let env_filter = if verbose {
        EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into())
    } else {
        EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into())
    };

    let builder = tracing_subscriber::fmt().with_env_filter(env_filter);
    if json_logs {
        builder.json().init();
    } else {
        builder.init();
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let json_logs = args.log_format.eq_ignore_ascii_case("json");
    init_tracing(args.verbose, json_logs);

    if let Some(t) = args.threads {
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
            .unwrap()
            .progress_chars("=>-"),
    );

    let config = organum::config::load_config();
    let files_to_process: Vec<PathBuf> = wav_files
        .into_iter()
        .filter(|wav| {
            if args.force {
                return true;
            }
            let sc_path = organum::resampler::to_feature_path(wav, &config.feature_extension);
            if sc_path.exists() && is_feature_cache_compatible(&sc_path) {
                pb.inc(1);
                false
            } else {
                true
            }
        })
        .collect();

    if files_to_process.is_empty() {
        pb.finish_with_message("All .wav files are already cached!");
        return Ok(());
    }

    let fail_count = AtomicUsize::new(0);

    files_to_process.par_iter().for_each(|wav_path| {
        if let Err(e) = generate_and_cache_features(wav_path, &config) {
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
