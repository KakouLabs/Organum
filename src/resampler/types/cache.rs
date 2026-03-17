use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::features::WorldFeatures;

#[derive(Clone)]
pub struct MatrixF64 {
    pub data: Vec<f64>,
    pub rows: usize,
    pub cols: usize,
}

impl MatrixF64 {
    pub fn from_flat(data: Vec<f64>, rows: usize, cols: usize) -> anyhow::Result<Self> {
        if data.len() != rows * cols {
            anyhow::bail!("MatrixF64: data length {} != {}x{}", data.len(), rows, cols);
        }
        Ok(Self { data, rows, cols })
    }

    pub fn from_vecs(vecs: &[Vec<f64>]) -> anyhow::Result<Self> {
        let rows = vecs.len();
        let cols = vecs.first().map_or(0, Vec::len);
        if vecs.iter().any(|row| row.len() != cols) {
            anyhow::bail!("MatrixF64: ragged rows are not allowed");
        }
        let mut data = Vec::with_capacity(rows * cols);
        for row in vecs {
            data.extend_from_slice(row);
        }
        Ok(Self { data, rows, cols })
    }

    #[inline]
    pub fn row(&self, i: usize) -> &[f64] {
        let start = i * self.cols;
        &self.data[start..start + self.cols]
    }

    pub fn to_vecs(&self) -> Vec<Vec<f64>> {
        self.data
            .chunks_exact(self.cols)
            .map(|c| c.to_vec())
            .collect()
    }

    pub fn byte_size(&self) -> usize {
        self.data.len() * std::mem::size_of::<f64>()
    }
}

#[derive(Clone)]
pub struct WorldFeaturesOwned {
    pub base_f0: f64,
    pub f0: Vec<f64>,
    pub mgc: MatrixF64,
    pub bap: MatrixF64,
}

impl WorldFeaturesOwned {
    pub fn byte_size(&self) -> usize {
        self.f0.len() * std::mem::size_of::<f64>() + self.mgc.byte_size() + self.bap.byte_size()
    }

    pub fn from_world_features(wf: &WorldFeatures) -> Self {
        Self {
            base_f0: wf.base_f0,
            f0: wf.f0.clone(),
            mgc: MatrixF64::from_vecs(&wf.mgc)
                .unwrap_or_else(|e| panic!("invalid mgc matrix: {e}")),
            bap: MatrixF64::from_vecs(&wf.bap)
                .unwrap_or_else(|e| panic!("invalid bap matrix: {e}")),
        }
    }

    pub fn to_world_features(&self) -> WorldFeatures {
        WorldFeatures {
            base_f0: self.base_f0,
            f0: self.f0.clone(),
            mgc: self.mgc.to_vecs(),
            bap: self.bap.to_vecs(),
        }
    }
}

pub const CACHE_V5_MAGIC: [u8; 4] = *b"OGN5";
pub const CACHE_V5_HEADER_SIZE: usize = 40;
pub const CACHE_V5_FORMAT_VERSION: u32 = 5;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct CacheV5Header {
    pub magic: [u8; 4],
    pub format_version: u32,
    pub cache_key: u64,
    pub sample_rate: u32,
    pub frame_period_micros: u32,
    pub frame_count: u32,
    pub mgc_dims: u16,
    pub bap_dims: u16,
    pub payload_size: u32,
    pub _reserved: u32,
}

const _: () = assert!(std::mem::size_of::<CacheV5Header>() == CACHE_V5_HEADER_SIZE);

impl CacheV5Header {
    pub fn to_bytes(&self) -> [u8; CACHE_V5_HEADER_SIZE] {
        let mut out = [0u8; CACHE_V5_HEADER_SIZE];
        out[0..4].copy_from_slice(&self.magic);
        out[4..8].copy_from_slice(&self.format_version.to_le_bytes());
        out[8..16].copy_from_slice(&self.cache_key.to_le_bytes());
        out[16..20].copy_from_slice(&self.sample_rate.to_le_bytes());
        out[20..24].copy_from_slice(&self.frame_period_micros.to_le_bytes());
        out[24..28].copy_from_slice(&self.frame_count.to_le_bytes());
        out[28..30].copy_from_slice(&self.mgc_dims.to_le_bytes());
        out[30..32].copy_from_slice(&self.bap_dims.to_le_bytes());
        out[32..36].copy_from_slice(&self.payload_size.to_le_bytes());
        out[36..40].copy_from_slice(&self._reserved.to_le_bytes());
        out
    }

    pub fn from_bytes(bytes: &[u8; CACHE_V5_HEADER_SIZE]) -> Self {
        Self {
            magic: bytes[0..4].try_into().unwrap_or([0; 4]),
            format_version: u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4])),
            cache_key: u64::from_le_bytes(bytes[8..16].try_into().unwrap_or([0; 8])),
            sample_rate: u32::from_le_bytes(bytes[16..20].try_into().unwrap_or([0; 4])),
            frame_period_micros: u32::from_le_bytes(bytes[20..24].try_into().unwrap_or([0; 4])),
            frame_count: u32::from_le_bytes(bytes[24..28].try_into().unwrap_or([0; 4])),
            mgc_dims: u16::from_le_bytes(bytes[28..30].try_into().unwrap_or([0; 2])),
            bap_dims: u16::from_le_bytes(bytes[30..32].try_into().unwrap_or([0; 2])),
            payload_size: u32::from_le_bytes(bytes[32..36].try_into().unwrap_or([0; 4])),
            _reserved: u32::from_le_bytes(bytes[36..40].try_into().unwrap_or([0; 4])),
        }
    }
}

pub fn compute_cache_key(sample_rate: u32, frame_period_micros: u32) -> u64 {
    let mut h = DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut h);
    CACHE_V5_FORMAT_VERSION.hash(&mut h);
    sample_rate.hash(&mut h);
    frame_period_micros.hash(&mut h);
    h.finish()
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DimQuantParams {
    pub bias: f32,
    pub scale: f32,
}

impl DimQuantParams {
    pub fn from_column(matrix: &MatrixF64, col: usize) -> Self {
        if matrix.rows == 0 {
            return Self {
                bias: 0.0,
                scale: 1.0,
            };
        }

        let mut min_val = f64::INFINITY;
        let mut max_val = f64::NEG_INFINITY;
        for r in 0..matrix.rows {
            let v = matrix.data[r * matrix.cols + col];
            if v < min_val {
                min_val = v;
            }
            if v > max_val {
                max_val = v;
            }
        }

        let range = max_val - min_val;
        if range < 1e-12 {
            return Self {
                bias: min_val as f32,
                scale: 1.0,
            };
        }

        let mid = (min_val + max_val) * 0.5;
        let half_range = range * 0.5;
        let scale = half_range / 32767.0;

        Self {
            bias: mid as f32,
            scale: scale as f32,
        }
    }

    #[inline]
    pub fn quantize(&self, value: f64) -> i16 {
        let q = ((value - self.bias as f64) / self.scale as f64).round();
        q.clamp(-32768.0, 32767.0) as i16
    }

    #[inline]
    pub fn dequantize(&self, q: i16) -> f64 {
        q as f64 * self.scale as f64 + self.bias as f64
    }
}

pub fn v5_payload_size(frame_count: usize, mgc_dims: usize, bap_dims: usize) -> usize {
    v5_payload_upper_bound(frame_count, mgc_dims, bap_dims)
}

pub fn v5_payload_upper_bound(frame_count: usize, mgc_dims: usize, bap_dims: usize) -> usize {
    4 + 4
        + 4
        + 2 * frame_count
        + 1
        + 8 * mgc_dims
        + 2 * frame_count * mgc_dims
        + 1
        + 8 * bap_dims
        + 2 * frame_count * bap_dims
}

pub fn encode_v5_payload(features: &WorldFeaturesOwned) -> Vec<u8> {
    let frame_count = features.f0.len();
    let mgc_dims = features.mgc.cols;
    let bap_dims = features.bap.cols;

    let mut out = Vec::with_capacity(v5_payload_upper_bound(frame_count, mgc_dims, bap_dims));

    out.extend_from_slice(&(features.base_f0 as f32).to_le_bytes());

    let (f0_base, f0_scale, f0_deltas) = f0_delta_encode_i16(&features.f0);
    out.extend_from_slice(&f0_base.to_le_bytes());
    out.extend_from_slice(&f0_scale.to_le_bytes());
    for d in f0_deltas {
        out.extend_from_slice(&d.to_le_bytes());
    }

    encode_matrix_adaptive(&features.mgc, &mut out);
    encode_matrix_adaptive(&features.bap, &mut out);

    out
}

pub fn decode_v5_payload(
    data: &[u8],
    frame_count: usize,
    mgc_dims: usize,
    bap_dims: usize,
) -> anyhow::Result<WorldFeaturesOwned> {
    let mut cursor = 0usize;
    let base_f0 = read_f32(data, &mut cursor)? as f64;

    let f0_base = read_f32(data, &mut cursor)?;
    let f0_scale = read_f32(data, &mut cursor)?;
    let f0 = f0_delta_decode_i16(f0_base, f0_scale, data, &mut cursor, frame_count)?;

    let mgc = decode_matrix_adaptive(data, &mut cursor, frame_count, mgc_dims)?;
    let bap = decode_matrix_adaptive(data, &mut cursor, frame_count, bap_dims)?;

    Ok(WorldFeaturesOwned {
        base_f0,
        f0,
        mgc,
        bap,
    })
}

fn f0_delta_encode_i16(f0: &[f64]) -> (f32, f32, Vec<i16>) {
    if f0.is_empty() {
        return (0.0, 1.0, Vec::new());
    }

    let f0_base = f0[0] as f32;
    let mut deltas = Vec::with_capacity(f0.len());
    let mut prev = f0_base;
    for &v in f0 {
        let cur = v as f32;
        deltas.push(cur - prev);
        prev = cur;
    }

    let max_abs = deltas.iter().fold(0.0_f32, |m, &v| m.max(v.abs()));
    let scale = if max_abs < 1e-12 {
        1.0
    } else {
        max_abs / 32767.0
    };

    let q = deltas
        .into_iter()
        .map(|d| (d / scale).round().clamp(-32768.0, 32767.0) as i16)
        .collect();

    (f0_base, scale, q)
}

fn f0_delta_decode_i16(
    f0_base: f32,
    f0_scale: f32,
    data: &[u8],
    cursor: &mut usize,
    frame_count: usize,
) -> anyhow::Result<Vec<f64>> {
    let mut out = Vec::with_capacity(frame_count);
    let mut acc = f0_base;
    for _ in 0..frame_count {
        let q = read_i16(data, cursor)?;
        acc += q as f32 * f0_scale;
        out.push(acc as f64);
    }
    Ok(out)
}

fn build_matrix_quantized_raw(matrix: &MatrixF64) -> (Vec<DimQuantParams>, Vec<i16>) {
    let dims = matrix.cols;
    let frames = matrix.rows;

    let params: Vec<DimQuantParams> = (0..dims)
        .map(|d| DimQuantParams::from_column(matrix, d))
        .collect();

    let mut q = Vec::with_capacity(frames * dims);
    for r in 0..frames {
        let row_start = r * dims;
        for (d, param) in params.iter().enumerate().take(dims) {
            q.push(param.quantize(matrix.data[row_start + d]));
        }
    }
    (params, q)
}

fn build_matrix_quantized_delta(matrix: &MatrixF64) -> (Vec<DimQuantParams>, Vec<i16>) {
    let dims = matrix.cols;
    let frames = matrix.rows;

    let mut delta = vec![0.0_f64; frames * dims];
    for d in 0..dims {
        let mut prev = 0.0_f64;
        for r in 0..frames {
            let idx = r * dims + d;
            let cur = matrix.data[idx];
            delta[idx] = if r == 0 { cur } else { cur - prev };
            prev = cur;
        }
    }

    let delta_matrix = MatrixF64 {
        data: delta,
        rows: frames,
        cols: dims,
    };

    let params: Vec<DimQuantParams> = (0..dims)
        .map(|d| DimQuantParams::from_column(&delta_matrix, d))
        .collect();

    let mut q = Vec::with_capacity(frames * dims);
    for r in 0..frames {
        let row_start = r * dims;
        for (d, param) in params.iter().enumerate().take(dims) {
            q.push(param.quantize(delta_matrix.data[row_start + d]));
        }
    }
    (params, q)
}

fn encode_matrix_quantized_with_parts(out: &mut Vec<u8>, params: &[DimQuantParams], q: &[i16]) {
    for p in params {
        out.extend_from_slice(&p.bias.to_le_bytes());
        out.extend_from_slice(&p.scale.to_le_bytes());
    }
    append_i16_slice_le(out, q);
}

#[inline]
fn matrix_delta_roughness(matrix: &MatrixF64) -> f64 {
    if matrix.rows < 2 || matrix.cols == 0 {
        return 0.0;
    }
    let mut acc = 0.0;
    let mut count = 0usize;
    for r in 1..matrix.rows {
        let prev = (r - 1) * matrix.cols;
        let cur = r * matrix.cols;
        for d in 0..matrix.cols {
            acc += (matrix.data[cur + d] - matrix.data[prev + d]).abs();
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        acc / count as f64
    }
}

fn encode_matrix_adaptive(matrix: &MatrixF64, out: &mut Vec<u8>) {
    const MODE_RAW: u8 = 0;
    const MODE_DELTA: u8 = 1;

    if matrix.rows <= 1 || matrix.cols == 0 {
        out.push(MODE_RAW);
        let (raw_params, raw_q) = build_matrix_quantized_raw(matrix);
        encode_matrix_quantized_with_parts(out, &raw_params, &raw_q);
        return;
    }

    let rough = matrix_delta_roughness(matrix);

    if rough < 0.10 {
        out.push(MODE_DELTA);
        let (delta_params, delta_q) = build_matrix_quantized_delta(matrix);
        encode_matrix_quantized_with_parts(out, &delta_params, &delta_q);
        return;
    }

    let (raw_params, raw_q) = build_matrix_quantized_raw(matrix);
    let (delta_params, delta_q) = build_matrix_quantized_delta(matrix);

    let raw_cost = estimate_quant_stream_cost(&raw_q);
    let delta_cost = estimate_quant_stream_cost(&delta_q);

    if delta_cost < raw_cost * 0.96 {
        out.push(MODE_DELTA);
        encode_matrix_quantized_with_parts(out, &delta_params, &delta_q);
    } else {
        out.push(MODE_RAW);
        encode_matrix_quantized_with_parts(out, &raw_params, &raw_q);
    }
}

#[inline]
fn estimate_quant_stream_cost(q: &[i16]) -> f64 {
    if q.is_empty() {
        return 0.0;
    }

    let mut near_zero = 0usize;
    let mut repeats = 0usize;
    let mut sign_changes = 0usize;
    let mut prev = q[0];

    for &v in q {
        if v.abs() <= 2 {
            near_zero += 1;
        }
        if v == prev {
            repeats += 1;
        }
        if (v < 0) != (prev < 0) {
            sign_changes += 1;
        }
        prev = v;
    }

    q.len() as f64 - near_zero as f64 * 0.70 - repeats as f64 * 0.35 + sign_changes as f64 * 0.10
}

fn decode_matrix_quantized(
    data: &[u8],
    cursor: &mut usize,
    frames: usize,
    dims: usize,
) -> anyhow::Result<MatrixF64> {
    let mut params = Vec::with_capacity(dims);
    for _ in 0..dims {
        let bias = read_f32(data, cursor)?;
        let scale = read_f32(data, cursor)?;
        params.push(DimQuantParams { bias, scale });
    }

    let q = read_i16_slice(data, cursor, frames * dims)?;
    let mut flat = Vec::with_capacity(frames * dims);
    for r in 0..frames {
        let row_start = r * dims;
        for d in 0..dims {
            flat.push(params[d].dequantize(q[row_start + d]));
        }
    }

    MatrixF64::from_flat(flat, frames, dims)
}

fn decode_matrix_quantized_delta(
    data: &[u8],
    cursor: &mut usize,
    frames: usize,
    dims: usize,
) -> anyhow::Result<MatrixF64> {
    let mut params = Vec::with_capacity(dims);
    for _ in 0..dims {
        let bias = read_f32(data, cursor)?;
        let scale = read_f32(data, cursor)?;
        params.push(DimQuantParams { bias, scale });
    }

    let q = read_i16_slice(data, cursor, frames * dims)?;
    let mut flat = vec![0.0; frames * dims];
    for (d, param) in params.iter().enumerate().take(dims) {
        let mut acc = 0.0;
        for r in 0..frames {
            let idx = r * dims + d;
            let delta = param.dequantize(q[idx]);
            acc = if r == 0 { delta } else { acc + delta };
            flat[idx] = acc;
        }
    }

    MatrixF64::from_flat(flat, frames, dims)
}

fn decode_matrix_adaptive(
    data: &[u8],
    cursor: &mut usize,
    frames: usize,
    dims: usize,
) -> anyhow::Result<MatrixF64> {
    if *cursor >= data.len() {
        anyhow::bail!("unexpected end of payload at offset {}", *cursor);
    }
    let mode = data[*cursor];
    *cursor += 1;

    match mode {
        0 => decode_matrix_quantized(data, cursor, frames, dims),
        1 => decode_matrix_quantized_delta(data, cursor, frames, dims),
        _ => anyhow::bail!("unknown matrix encoding mode {}", mode),
    }
}

#[inline]
fn append_i16_slice_le(out: &mut Vec<u8>, values: &[i16]) {
    let byte_len = std::mem::size_of_val(values);
    out.reserve(byte_len);
    if cfg!(target_endian = "little") {
        let ptr = values.as_ptr() as *const u8;
        unsafe { out.extend_from_slice(std::slice::from_raw_parts(ptr, byte_len)) };
    } else {
        for &v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

fn read_i16_slice(data: &[u8], cursor: &mut usize, count: usize) -> anyhow::Result<Vec<i16>> {
    let bytes_len = count * 2;
    if *cursor + bytes_len > data.len() {
        anyhow::bail!("unexpected end of payload at offset {}", *cursor);
    }

    let bytes = &data[*cursor..*cursor + bytes_len];
    *cursor += bytes_len;

    if cfg!(target_endian = "little") {
        let mut out = vec![0i16; count];
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.as_mut_ptr() as *mut u8, bytes_len);
        }
        Ok(out)
    } else {
        let mut out = Vec::with_capacity(count);
        for chunk in bytes.chunks_exact(2) {
            out.push(i16::from_le_bytes([chunk[0], chunk[1]]));
        }
        Ok(out)
    }
}

#[inline]
fn read_f32(data: &[u8], cursor: &mut usize) -> anyhow::Result<f32> {
    if *cursor + 4 > data.len() {
        anyhow::bail!("unexpected end of payload at offset {}", *cursor);
    }
    let bytes: [u8; 4] = data[*cursor..*cursor + 4].try_into().unwrap();
    *cursor += 4;
    Ok(f32::from_le_bytes(bytes))
}

#[inline]
fn read_i16(data: &[u8], cursor: &mut usize) -> anyhow::Result<i16> {
    if *cursor + 2 > data.len() {
        anyhow::bail!("unexpected end of payload at offset {}", *cursor);
    }
    let bytes: [u8; 2] = data[*cursor..*cursor + 2].try_into().unwrap();
    *cursor += 2;
    Ok(i16::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn matrix_roundtrip() {
        let src = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let m = MatrixF64::from_vecs(&src).expect("matrix should be valid");
        assert_eq!(m.rows, 2);
        assert_eq!(m.cols, 2);
        assert_eq!(m.to_vecs(), src);
    }

    #[test]
    fn payload_roundtrip() {
        let f = WorldFeaturesOwned {
            base_f0: 440.0,
            f0: vec![440.0, 442.0, 445.0],
            mgc: MatrixF64::from_vecs(&vec![
                vec![0.1, 0.2, 0.3],
                vec![0.15, 0.25, 0.35],
                vec![0.2, 0.3, 0.4],
            ])
            .expect("mgc matrix should be valid"),
            bap: MatrixF64::from_vecs(&vec![
                vec![-0.1, -0.2],
                vec![-0.15, -0.25],
                vec![-0.2, -0.3],
            ])
            .expect("bap matrix should be valid"),
        };

        let p = encode_v5_payload(&f);
        let d = decode_v5_payload(&p, 3, 3, 2).unwrap();

        assert!(approx_eq(d.base_f0, f.base_f0, 0.1));
        for (a, b) in d.f0.iter().zip(f.f0.iter()) {
            assert!(approx_eq(*a, *b, 0.5));
        }
    }

    #[test]
    fn matrix_from_vecs_rejects_ragged_rows() {
        let ragged = vec![vec![1.0, 2.0], vec![3.0]];
        assert!(MatrixF64::from_vecs(&ragged).is_err());
    }
}
