use organum::wavtool::{concatenate, AudioPart, EnvPoint, WavtoolRequest};
use std::env;
use std::fs;
use std::path::Path;

fn get_wav_duration_ms(path: &Path) -> f32 {
    if let Ok(reader) = hound::WavReader::open(path) {
        let spec = reader.spec();
        let samples = reader.duration();
        samples as f32 / spec.sample_rate as f32 * 1000.0
    } else {
        0.0
    }
}

fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().collect();

    // Check for JSON mode
    if args.len() == 3 && args[1] == "--json" {
        let json_str = fs::read_to_string(&args[2]).expect("Failed to read JSON file");
        let req: WavtoolRequest = serde_json::from_str(&json_str).unwrap();
        if let Err(e) = concatenate(&req) {
            eprintln!("Error joining audio: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if args.len() < 5 {
        eprintln!("Usage: wavtool <outfile> <infile> <skip_ms> <length_ms> [p1 p2 p3] [v1 v2 v3 v4] [overlap] [consonant] [blank]");
        std::process::exit(1);
    }

    let outfile = args[1].clone();
    let infile = args[2].clone();

    let skip_ms: f32 = args[3].parse().unwrap_or(0.0);
    let len_raw = args.get(4).cloned().unwrap_or_else(|| "0".to_string());

    let get_arg = |i: usize| -> f32 { args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.0) };

    let p1 = get_arg(5);
    let p2 = get_arg(6);
    let p3 = get_arg(7);
    let v1 = get_arg(8);
    let v2 = get_arg(9);
    let v3 = get_arg(10);
    let v4 = if args.len() > 11 { get_arg(11) } else { 0.0 };
    let ovr = if args.len() > 12 { get_arg(12) } else { 0.0 };
    let _p4 = if args.len() > 13 { get_arg(13) } else { 0.0 };
    let p5 = if args.len() > 14 { get_arg(14) } else { 0.0 };
    let v5 = if args.len() > 15 { get_arg(15) } else { 0.0 };

    let length_ms = organum::utils::parse_utau_length(&len_raw, 120.0);
    let overlap = ovr;

    let out_path = Path::new(&outfile);
    let current_duration = get_wav_duration_ms(out_path);

    let offset_ms = (current_duration - overlap).max(0.0);

    // Parse envelope
    let mut envelope = Vec::new();
    let mut t = p1;
    envelope.push(EnvPoint {
        time_ms: t,
        volume: v1 / 100.0,
    });
    t += p2;
    envelope.push(EnvPoint {
        time_ms: t,
        volume: v2 / 100.0,
    });

    if args.len() > 14 {
        t += p5;
        envelope.push(EnvPoint {
            time_ms: t,
            volume: v5 / 100.0,
        });
    }

    if length_ms > 0.0 {
        let end_t = length_ms;
        envelope.push(EnvPoint {
            time_ms: (end_t - p3).max(t),
            volume: v3 / 100.0,
        });
        envelope.push(EnvPoint {
            time_ms: end_t,
            volume: v4 / 100.0,
        });
    }

    // Sort envelope points to be valid
    envelope.sort_by(|a, b| a.time_ms.partial_cmp(&b.time_ms).unwrap());

    let req = WavtoolRequest {
        output_file: outfile,
        parts: vec![AudioPart {
            file_path: infile,
            offset_ms,
            skip_ms,
            length_ms,
            fade_in_ms: Some(p2),  // fallback fade
            fade_out_ms: Some(p3), // fallback fade
            volume: Some(1.0),
            envelope: Some(envelope),
        }],
    };

    if let Err(e) = concatenate(&req) {
        eprintln!("Error joining audio: {:?}", e);
        std::process::exit(1);
    }
}
