use organum::resampler::{resample, ResampleRequest};
use std::env;
use std::fs;
use std::path::Path;
use tracing_subscriber::EnvFilter;

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

fn parse_runtime_log_options(raw_args: Vec<String>) -> (Vec<String>, bool, bool) {
    let mut args = vec![raw_args[0].clone()];
    let mut verbose = false;
    let mut json_logs = false;

    let mut i = 1;
    while i < raw_args.len() {
        match raw_args[i].as_str() {
            "--verbose" => {
                verbose = true;
                i += 1;
            }
            "--log-format" if i + 1 < raw_args.len() => {
                json_logs = raw_args[i + 1].eq_ignore_ascii_case("json");
                i += 2;
            }
            _ => {
                args.extend(raw_args[i..].iter().cloned());
                break;
            }
        }
    }

    (args, verbose, json_logs)
}

fn main() {
    let raw_args: Vec<String> = env::args().collect();
    let (args, verbose, json_logs) = parse_runtime_log_options(raw_args);
    init_tracing(verbose, json_logs);

    // Check if JSON mode
    if args.len() == 3 && args[1] == "--json" {
        let json_str = fs::read_to_string(&args[2]).expect("Failed to read JSON file");
        let req: ResampleRequest = serde_json::from_str(&json_str).unwrap();
        if let Err(e) = resample(&req) {
            eprintln!("Error resampling: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if args.len() < 5 {
        eprintln!("Usage: resampler input output pitch velocity [flags offset length_req fixed_length end_blank volume modulation !tempo pitchbend]");
        std::process::exit(1);
    }

    let to_absolute = |p: &str| -> String {
        let path = Path::new(p);
        if path.is_absolute() {
            p.to_string()
        } else {
            env::current_dir()
                .map(|c| c.join(path).to_string_lossy().to_string())
                .unwrap_or_else(|_| p.to_string())
        }
    };

    let input_file = to_absolute(&args[1]);
    let output_file = to_absolute(&args[2]);
    let tone = args[3].clone();
    let velocity: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(100.0);

    let flags_arg = args.get(5).cloned().unwrap_or_default();
    let flags = if flags_arg == "-" || flags_arg == "\"\"" {
        "".to_string()
    } else {
        flags_arg
    };

    let offset: f32 = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let length_req: f32 = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(0.0);

    let fixed_length: f32 = args.get(8).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let cutoff: f32 = args.get(9).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let _volume: f32 = args.get(10).and_then(|s| s.parse().ok()).unwrap_or(100.0);
    let modulation: f32 = args.get(11).and_then(|s| s.parse().ok()).unwrap_or(0.0);

    let tempo_raw = args.get(12).cloned().unwrap_or_else(|| "!120".to_string());
    let pitchbend_raw = args.get(13).cloned().unwrap_or_default();

    let (tempo, pitchbends) = organum::utils::parse_pitchbend(&tempo_raw, &pitchbend_raw);

    let mut actual_flags = flags;
    if modulation != 0.0 {
        actual_flags.push_str(&format!("M{}", modulation));
    }

    let req = ResampleRequest {
        input_file: input_file.clone(),
        output_file: output_file.clone(),
        tone: tone.clone(),
        velocity,
        flags: actual_flags,
        offset,
        length_req,
        fixed_length,
        cutoff,
        tempo,
        base_tone: tone,
        pitchbend: if pitchbends.is_empty() {
            None
        } else {
            Some(pitchbends)
        },
    };

    let config = organum::config::load_config();
    let _feature_path =
        organum::resampler::to_feature_path(Path::new(&input_file), &config.feature_extension);

    if let Err(e) = resample(&req) {
        eprintln!("Error resampling: {:?}", e);
        std::process::exit(1);
    }
}
