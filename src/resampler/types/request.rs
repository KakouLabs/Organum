use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ResampleRequest {
    pub input_file: String,
    pub output_file: String,
    pub tone: String,
    pub velocity: f32,
    pub flags: String,
    pub offset: f32,
    pub length_req: f32,
    #[serde(default)]
    pub fixed_length: f32,
    #[serde(default)]
    pub cutoff: f32,
    pub tempo: f32,
    #[serde(default)]
    pub base_tone: String,
    #[serde(default)]
    pub pitchbend: Option<Vec<i16>>,
}
