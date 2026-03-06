use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Deserialize)]
pub struct WorldFeatures {
    pub base_f0: f64,
    pub f0: Vec<f64>,
    pub mgc: Vec<Vec<f64>>,
    pub bap: Vec<Vec<f64>>,
}

/// Cache payload for feature file version 4.
/// v4 uses strategy enum and lets encoder pick smaller compressed payload.
#[derive(Serialize, Deserialize)]
pub enum FeatureCacheV4 {
    Quantized(FeatureCacheV4Quantized),
    DeltaQuantized(FeatureCacheV4Delta),
}

#[derive(Serialize, Deserialize)]
pub struct FeatureCacheV4Quantized {
    pub base_f0: f32,
    pub f0: Vec<f32>,
    pub mgc_frames: u32,
    pub mgc_dims: u16,
    pub mgc_scale: f32,
    pub mgc_data: Vec<i16>,
    pub bap_frames: u32,
    pub bap_dims: u16,
    pub bap_scale: f32,
    pub bap_data: Vec<i16>,
}

#[derive(Serialize, Deserialize)]
pub struct FeatureCacheV4Delta {
    pub base_f0: f32,
    pub f0: Vec<f32>,
    pub mgc_frames: u32,
    pub mgc_dims: u16,
    pub mgc_delta_scale: f32,
    pub mgc_delta_data: Vec<i16>,
    pub bap_frames: u32,
    pub bap_dims: u16,
    pub bap_delta_scale: f32,
    pub bap_delta_data: Vec<i16>,
}

fn flatten_2d(data: &[Vec<f64>]) -> (usize, usize, Vec<f32>) {
    let frames = data.len();
    let dims = data.first().map_or(0, Vec::len);
    let flat = data
        .iter()
        .flat_map(|row| row.iter().map(|&v| v as f32))
        .collect();
    (frames, dims, flat)
}

fn reshape_2d(flat: Vec<f32>, frames: usize, dims: usize) -> anyhow::Result<Vec<Vec<f64>>> {
    if flat.len() != frames.saturating_mul(dims) {
        anyhow::bail!(
            "Invalid cache payload: data length {} does not match {}x{}",
            flat.len(),
            frames,
            dims
        );
    }
    Ok(flat
        .chunks(dims.max(1))
        .take(frames)
        .map(|row| row.iter().map(|&v| v as f64).collect())
        .collect())
}

fn quantize_i16(data: &[f32]) -> (f32, Vec<i16>) {
    let max_abs = data.iter().fold(0.0_f32, |m, &v| m.max(v.abs()));
    if max_abs <= 1e-12 {
        return (1.0, vec![0; data.len()]);
    }

    let scale = max_abs / 32767.0;
    let q = data
        .iter()
        .map(|&v| (v / scale).round().clamp(-32768.0, 32767.0) as i16)
        .collect();
    (scale, q)
}

fn dequantize_i16(scale: f32, data: &[i16]) -> Vec<f32> {
    data.iter().map(|&q| q as f32 * scale).collect()
}

fn delta_encode(data: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(data.len());
    let mut prev = 0.0_f32;
    for &x in data {
        out.push(x - prev);
        prev = x;
    }
    out
}

fn delta_decode(deltas: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(deltas.len());
    let mut acc = 0.0_f32;
    for &d in deltas {
        acc += d;
        out.push(acc);
    }
    out
}

impl From<&WorldFeatures> for FeatureCacheV4Quantized {
    fn from(features: &WorldFeatures) -> Self {
        let (mgc_frames, mgc_dims, mgc_flat) = flatten_2d(&features.mgc);
        let (bap_frames, bap_dims, bap_flat) = flatten_2d(&features.bap);
        let (mgc_scale, mgc_data) = quantize_i16(&mgc_flat);
        let (bap_scale, bap_data) = quantize_i16(&bap_flat);

        Self {
            base_f0: features.base_f0 as f32,
            f0: features.f0.iter().map(|&v| v as f32).collect(),
            mgc_frames: mgc_frames as u32,
            mgc_dims: mgc_dims as u16,
            mgc_scale,
            mgc_data,
            bap_frames: bap_frames as u32,
            bap_dims: bap_dims as u16,
            bap_scale,
            bap_data,
        }
    }
}

impl TryFrom<FeatureCacheV4Quantized> for WorldFeatures {
    type Error = anyhow::Error;

    fn try_from(cache: FeatureCacheV4Quantized) -> Result<Self, Self::Error> {
        let mgc_frames = cache.mgc_frames as usize;
        let mgc_dims = cache.mgc_dims as usize;
        let bap_frames = cache.bap_frames as usize;
        let bap_dims = cache.bap_dims as usize;

        if cache.mgc_data.len() != mgc_frames.saturating_mul(mgc_dims) {
            anyhow::bail!(
                "Invalid v4 quantized cache: mgc_data length {} does not match {}x{}",
                cache.mgc_data.len(),
                mgc_frames,
                mgc_dims
            );
        }
        if cache.bap_data.len() != bap_frames.saturating_mul(bap_dims) {
            anyhow::bail!(
                "Invalid v4 quantized cache: bap_data length {} does not match {}x{}",
                cache.bap_data.len(),
                bap_frames,
                bap_dims
            );
        }

        let mgc = reshape_2d(
            dequantize_i16(cache.mgc_scale, &cache.mgc_data),
            mgc_frames,
            mgc_dims,
        )?;
        let bap = reshape_2d(
            dequantize_i16(cache.bap_scale, &cache.bap_data),
            bap_frames,
            bap_dims,
        )?;

        Ok(Self {
            base_f0: cache.base_f0 as f64,
            f0: cache.f0.into_iter().map(|v| v as f64).collect(),
            mgc,
            bap,
        })
    }
}

impl From<&WorldFeatures> for FeatureCacheV4Delta {
    fn from(features: &WorldFeatures) -> Self {
        let (mgc_frames, mgc_dims, mgc_flat) = flatten_2d(&features.mgc);
        let (bap_frames, bap_dims, bap_flat) = flatten_2d(&features.bap);

        let mgc_deltas = delta_encode(&mgc_flat);
        let bap_deltas = delta_encode(&bap_flat);
        let (mgc_delta_scale, mgc_delta_data) = quantize_i16(&mgc_deltas);
        let (bap_delta_scale, bap_delta_data) = quantize_i16(&bap_deltas);

        Self {
            base_f0: features.base_f0 as f32,
            f0: features.f0.iter().map(|&v| v as f32).collect(),
            mgc_frames: mgc_frames as u32,
            mgc_dims: mgc_dims as u16,
            mgc_delta_scale,
            mgc_delta_data,
            bap_frames: bap_frames as u32,
            bap_dims: bap_dims as u16,
            bap_delta_scale,
            bap_delta_data,
        }
    }
}

impl TryFrom<FeatureCacheV4Delta> for WorldFeatures {
    type Error = anyhow::Error;

    fn try_from(cache: FeatureCacheV4Delta) -> Result<Self, Self::Error> {
        let mgc_frames = cache.mgc_frames as usize;
        let mgc_dims = cache.mgc_dims as usize;
        let bap_frames = cache.bap_frames as usize;
        let bap_dims = cache.bap_dims as usize;

        if cache.mgc_delta_data.len() != mgc_frames.saturating_mul(mgc_dims) {
            anyhow::bail!(
                "Invalid v4 delta cache: mgc_delta_data length {} does not match {}x{}",
                cache.mgc_delta_data.len(),
                mgc_frames,
                mgc_dims
            );
        }
        if cache.bap_delta_data.len() != bap_frames.saturating_mul(bap_dims) {
            anyhow::bail!(
                "Invalid v4 delta cache: bap_delta_data length {} does not match {}x{}",
                cache.bap_delta_data.len(),
                bap_frames,
                bap_dims
            );
        }

        let mgc_d = dequantize_i16(cache.mgc_delta_scale, &cache.mgc_delta_data);
        let bap_d = dequantize_i16(cache.bap_delta_scale, &cache.bap_delta_data);
        let mgc = reshape_2d(delta_decode(&mgc_d), mgc_frames, mgc_dims)?;
        let bap = reshape_2d(delta_decode(&bap_d), bap_frames, bap_dims)?;

        Ok(Self {
            base_f0: cache.base_f0 as f64,
            f0: cache.f0.into_iter().map(|v| v as f64).collect(),
            mgc,
            bap,
        })
    }
}

impl TryFrom<FeatureCacheV4> for WorldFeatures {
    type Error = anyhow::Error;

    fn try_from(cache: FeatureCacheV4) -> Result<Self, Self::Error> {
        match cache {
            FeatureCacheV4::Quantized(v) => v.try_into(),
            FeatureCacheV4::DeltaQuantized(v) => v.try_into(),
        }
    }
}
