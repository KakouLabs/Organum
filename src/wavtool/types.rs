use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct WavtoolRequest {
    pub output_file: String,
    pub parts: Vec<AudioPart>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EnvPoint {
    pub time_ms: f32,
    pub volume: f32,
}

#[derive(Debug, Deserialize)]
pub struct AudioPart {
    pub file_path: String,
    pub offset_ms: f32, // Where to place audio in the output (ms)
    pub skip_ms: f32,   // How much to skip from the start of input (ms)
    pub length_ms: f32, // How long the audio should be (ms)
    pub fade_in_ms: Option<f32>,
    pub fade_out_ms: Option<f32>,
    pub volume: Option<f32>,
    pub envelope: Option<Vec<EnvPoint>>,
}
