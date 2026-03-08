use organum::resampler::{resample, ResampleRequest};
use std::env;
use std::fs;
use std::path::Path;
use tracing_subscriber::EnvFilter;

const RESAMPLER_USAGE: &str = "Usage:\n  organum-resampler [--verbose] [--log-format pretty|json] <input> <output> <pitch> <velocity> [flags offset length_req fixed_length end_blank volume modulation !tempo pitchbend]\n\nExamples:\n  organum-resampler input.wav output.wav C4 100\n  organum-resampler input.wav output.wav C4 100 g+10B60A120 0 480 0 0 100 30 !120 #5#10#0\n  organum-resampler --json request.json\n\nNotes:\n  - Flags are case-insensitive. Common flags: g, B, M, t, A, P, C, H, D, F\n  - If flags is empty, use '-'\n  - tempo should look like '!120'\n";

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

fn warn_unknown_flags(flags: &str) {
    let supported = ['g', 'b', 'm', 't', 'a', 'p', 'c', 'h', 'd', 'f', 'y'];
    for c in flags.chars().filter(|c| c.is_ascii_alphabetic()) {
        if !supported.contains(&c.to_ascii_lowercase()) {
            eprintln!(
                "Warning: unknown flag '{}' ignored. Supported flags: g/B/M/t/A/P/C/H/D/F/y",
                c
            );
        }
    }
}

fn parse_f32_arg_or_exit(args: &[String], idx: usize, name: &str, default: f32, hint: &str) -> f32 {
    match args.get(idx) {
        Some(raw) => match raw.parse::<f32>() {
            Ok(v) => v,
            Err(_) => {
                eprintln!(
                    "Invalid {} '{}'. {}\n\n{}",
                    name, raw, hint, RESAMPLER_USAGE
                );
                std::process::exit(1);
            }
        },
        None => default,
    }
}

fn warn_if_out_of_range(name: &str, value: f32, min: f32, max: f32) {
    if !(min..=max).contains(&value) {
        eprintln!(
            "Warning: {} {} is outside recommended range {}~{}.",
            name, value, min, max
        );
    }
}

fn parse_json_mode(args: &[String]) -> bool {
    if args.len() == 3 && args[1] == "--json" {
        let json_str = fs::read_to_string(&args[2]).expect("Failed to read JSON file");
        let req: ResampleRequest = serde_json::from_str(&json_str).unwrap();
        if let Err(e) = resample(&req) {
            eprintln!("Error resampling: {}", e);
            std::process::exit(1);
        }
        return true;
    }

    if args.get(1).map(String::as_str) == Some("--json") {
        eprintln!(
            "Invalid --json invocation. Expected: organum-resampler --json <request.json>\n\n{}",
            RESAMPLER_USAGE
        );
        std::process::exit(1);
    }

    false
}

fn main() {
    let raw_args: Vec<String> = env::args().collect();
    let (args, verbose, json_logs) = parse_runtime_log_options(raw_args);
    init_tracing(verbose, json_logs);

    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        println!("{}", RESAMPLER_USAGE);
        return;
    }

    if parse_json_mode(&args) {
        return;
    }

    if args.len() < 5 {
        eprintln!("{}", RESAMPLER_USAGE);
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
    let velocity = parse_f32_arg_or_exit(
        &args,
        4,
        "velocity",
        100.0,
        "Expected a number, typically 0~200.",
    );
    warn_if_out_of_range("velocity", velocity, 0.0, 200.0);

    let flags_arg = args.get(5).cloned().unwrap_or_default();
    let flags = if flags_arg == "-" || flags_arg == "\"\"" {
        "".to_string()
    } else {
        flags_arg
    };
    if !flags.is_empty() {
        warn_unknown_flags(&flags);
    }

    let offset = parse_f32_arg_or_exit(&args, 6, "offset", 0.0, "Expected milliseconds as number.");
    let length_req = parse_f32_arg_or_exit(
        &args,
        7,
        "length_req",
        0.0,
        "Expected milliseconds as number.",
    );
    let fixed_length = parse_f32_arg_or_exit(
        &args,
        8,
        "fixed_length",
        0.0,
        "Expected milliseconds as number.",
    );
    let cutoff = parse_f32_arg_or_exit(
        &args,
        9,
        "end_blank",
        0.0,
        "Expected milliseconds as number.",
    );
    let _volume: f32 = args.get(10).and_then(|s| s.parse().ok()).unwrap_or(100.0);
    let modulation = parse_f32_arg_or_exit(
        &args,
        11,
        "modulation",
        0.0,
        "Expected a number, typically 0~100.",
    );
    warn_if_out_of_range("modulation", modulation, 0.0, 100.0);

    let tempo_raw = args.get(12).cloned().unwrap_or_else(|| "!120".to_string());
    if !tempo_raw.starts_with('!') {
        eprintln!(
            "Invalid tempo '{}'. Tempo should start with '!' (example: !120).\n\n{}",
            tempo_raw, RESAMPLER_USAGE
        );
        std::process::exit(1);
    }
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
