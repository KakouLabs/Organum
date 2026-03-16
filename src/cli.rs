use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;
use tracing_subscriber::EnvFilter;

pub fn init_tracing(verbose: bool, json_logs: bool) {
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

pub fn parse_runtime_log_options(raw_args: Vec<String>) -> (Vec<String>, bool, bool) {
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

pub fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let json_str = fs::read_to_string(path)
        .with_context(|| format!("Failed to read JSON file: {}", path.display()))?;
    serde_json::from_str(&json_str)
        .with_context(|| format!("Failed to parse JSON file: {}", path.display()))
}
