use crate::resampler::common::consts;
use crate::resampler::types::MatrixF64;
use rayon::prelude::*;
use std::path::Path;

pub use crate::utils::{midi_to_hz, note_to_midi};

pub struct LinearInterpolator<'a> {
    pub curve: &'a [f64],
}

impl<'a> LinearInterpolator<'a> {
    #[inline(always)]
    pub fn new(curve: &'a [f64]) -> Self {
        Self { curve }
    }

    #[inline(always)]
    pub fn sample(&self, x: f64) -> f64 {
        let len = self.curve.len();
        if len == 0 {
            return 0.0;
        }
        if len == 1 {
            return self.curve[0];
        }
        let last = len - 1;
        if x <= 0.0 {
            return self.curve[0];
        }
        if x >= last as f64 {
            return self.curve[last];
        }
        let index = x as usize;
        let r = x - index as f64;
        unsafe {
            self.curve.get_unchecked(index) * (1.0 - r) + self.curve.get_unchecked(index + 1) * r
        }
    }

    pub fn sample_vec(&self, xs: &[f64]) -> Vec<f64> {
        xs.par_iter().map(|&x| self.sample(x)).collect()
    }

    pub fn sample_vec_adaptive(&self, xs: &[f64]) -> Vec<f64> {
        if xs.len() < 2048 {
            xs.iter().map(|&x| self.sample(x)).collect()
        } else {
            xs.par_iter().map(|&x| self.sample(x)).collect()
        }
    }
}

pub struct CubicSplineInterpolator<'a> {
    pub curve: &'a [f64],
}

impl<'a> CubicSplineInterpolator<'a> {
    #[inline(always)]
    pub fn new(curve: &'a [f64]) -> Self {
        Self { curve }
    }

    #[inline(always)]
    pub fn sample(&self, x: f64) -> f64 {
        crate::utils::cubic_interpolate_f64(self.curve, x)
    }

    pub fn sample_vec(&self, xs: &[f64]) -> Vec<f64> {
        xs.par_iter().map(|&x| self.sample(x)).collect()
    }

    pub fn sample_vec_adaptive(&self, xs: &[f64]) -> Vec<f64> {
        if xs.len() < 2048 {
            xs.iter().map(|&x| self.sample(x)).collect()
        } else {
            xs.par_iter().map(|&x| self.sample(x)).collect()
        }
    }
}

pub fn interpolate_frames(vec_2d: &[Vec<f64>], points: &[f64]) -> Vec<Vec<f64>> {
    let src = MatrixF64::from_vecs(vec_2d)
        .unwrap_or_else(|_| MatrixF64::from_flat(Vec::new(), 0, 0).expect("empty matrix is valid"));
    interpolate_frames_matrix(&src, points).to_vecs()
}

pub fn interpolate_frames_matrix(src: &MatrixF64, points: &[f64]) -> MatrixF64 {
    if src.rows == 0 || src.cols == 0 {
        return MatrixF64 {
            data: Vec::new(),
            rows: 0,
            cols: 0,
        };
    }

    let n_frames = src.rows;
    let n_dims = src.cols;
    let out_len = points.len();
    let mut out = vec![0.0; out_len * n_dims];
    let last = (n_frames - 1) as f64;

    let fill_frame = |out_frame: &mut [f64], p: f64| {
        if p <= 0.0 {
            out_frame.copy_from_slice(&src.data[..n_dims]);
        } else if p >= last {
            let start = (n_frames - 1) * n_dims;
            out_frame.copy_from_slice(&src.data[start..start + n_dims]);
        } else {
            let idx = p as usize;
            let frac = p - idx as f64;
            let a_start = idx * n_dims;
            let b_start = (idx + 1) * n_dims;
            let frame_a = &src.data[a_start..a_start + n_dims];
            let frame_b = &src.data[b_start..b_start + n_dims];
            for d in 0..n_dims {
                let a = frame_a[d];
                out_frame[d] = a + (frame_b[d] - a) * frac;
            }
        }
    };

    const PAR_THRESHOLD: usize = 1536;
    if out_len < PAR_THRESHOLD {
        out.chunks_mut(n_dims)
            .zip(points.iter().copied())
            .for_each(|(dst, p)| fill_frame(dst, p));
    } else {
        out.par_chunks_mut(n_dims)
            .zip(points.par_iter().copied())
            .for_each(|(dst, p)| fill_frame(dst, p));
    }

    MatrixF64 {
        data: out,
        rows: out_len,
        cols: n_dims,
    }
}

pub fn to_feature_path(path: &Path, ext: &str) -> std::path::PathBuf {
    let ext_str = format!("wav.{}", ext);
    path.with_extension(ext_str)
}

pub fn parse_pitchbend_to_semitones(pitchbend: &Option<Vec<i16>>) -> Vec<f64> {
    match pitchbend {
        Some(pb) if !pb.is_empty() => pb.iter().map(|&v| v as f64 / 100.0).collect(),
        _ => vec![0.0],
    }
}

pub fn calculate_base_f0(f0: &[f64]) -> f64 {
    let mut tally = 0.0;

    let base_f0: f64 = f0
        .iter()
        .enumerate()
        .filter(|(_, &freq)| (consts::F0_FLOOR..=consts::F0_CEIL).contains(&freq))
        .map(|(i, &freq)| {
            let neighbor_diff = if i == 0 {
                f0.get(1).unwrap_or(&freq) - freq
            } else if i == f0.len() - 1 {
                f0.get(i - 1).unwrap_or(&freq) - freq
            } else {
                0.5 * (f0[i + 1] - f0[i - 1])
            };

            let weight = (-neighbor_diff * neighbor_diff).exp2();
            tally += weight;
            freq * weight
        })
        .sum();

    if tally > 0.0 {
        base_f0 / tally
    } else {
        440.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_interpolator() {
        let curve = vec![0.0, 10.0, 20.0, 30.0];
        let interp = LinearInterpolator::new(&curve);

        assert_eq!(interp.sample(-1.0), 0.0); // out of bounds (left)
        assert_eq!(interp.sample(0.0), 0.0);
        assert_eq!(interp.sample(0.5), 5.0); // half way between 0.0 and 10.0
        assert_eq!(interp.sample(1.0), 10.0);
        assert_eq!(interp.sample(1.5), 15.0);
        assert_eq!(interp.sample(3.0), 30.0);
        assert_eq!(interp.sample(4.0), 30.0); // out of bounds (right)

        let curve_empty: Vec<f64> = vec![];
        let interp_empty = LinearInterpolator::new(&curve_empty);
        assert_eq!(interp_empty.sample(1.0), 0.0);

        let curve_one = vec![42.0];
        let interp_one = LinearInterpolator::new(&curve_one);
        assert_eq!(interp_one.sample(0.0), 42.0);
        assert_eq!(interp_one.sample(1.0), 42.0);
    }

    #[test]
    fn test_interpolate_frames() {
        let frames = vec![vec![1.0, 2.0], vec![2.0, 4.0], vec![3.0, 6.0]];

        let points = vec![-1.0, 0.0, 0.5, 1.0, 2.0, 3.0];
        let res = interpolate_frames(&frames, &points);

        assert_eq!(res.len(), 6);
        assert_eq!(res[0], vec![1.0, 2.0]); // p <= 0
        assert_eq!(res[1], vec![1.0, 2.0]);
        assert_eq!(res[2], vec![1.5, 3.0]); // half way
        assert_eq!(res[3], vec![2.0, 4.0]);
        assert_eq!(res[4], vec![3.0, 6.0]);
        assert_eq!(res[5], vec![3.0, 6.0]); // p >= last
    }

    #[test]
    fn test_parse_pitchbend() {
        assert_eq!(parse_pitchbend_to_semitones(&None), vec![0.0]);
        assert_eq!(parse_pitchbend_to_semitones(&Some(vec![])), vec![0.0]);
        assert_eq!(
            parse_pitchbend_to_semitones(&Some(vec![0, 100, -200])),
            vec![0.0, 1.0, -2.0]
        );
    }
}
