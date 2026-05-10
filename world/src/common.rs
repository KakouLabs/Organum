//! Common types and utilities for the WORLD crate.

#[derive(Clone, Debug, PartialEq)]
pub struct MatrixF32 {
    rows: usize,
    cols: usize,
    data: Vec<f32>,
}

impl MatrixF32 {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows.saturating_mul(cols)],
        }
    }

    pub fn from_vec(rows: usize, cols: usize, data: Vec<f32>) -> Self {
        assert_eq!(data.len(), rows.saturating_mul(cols));
        Self { rows, cols, data }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }

    pub fn into_vec(self) -> Vec<f32> {
        self.data
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.rows = rows;
        self.cols = cols;
        self.data.resize(rows.saturating_mul(cols), 0.0);
    }

    pub fn fill(&mut self, value: f32) {
        self.data.fill(value);
    }

    pub fn row(&self, row: usize) -> &[f32] {
        assert!(row < self.rows);
        let start = row * self.cols;
        &self.data[start..start + self.cols]
    }

    pub fn row_mut(&mut self, row: usize) -> &mut [f32] {
        assert!(row < self.rows);
        let start = row * self.cols;
        &mut self.data[start..start + self.cols]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatrixViewF32<'a> {
    rows: usize,
    cols: usize,
    data: &'a [f32],
}

impl<'a> MatrixViewF32<'a> {
    pub fn new(data: &'a [f32], rows: usize, cols: usize) -> Self {
        assert_eq!(data.len(), rows.saturating_mul(cols));
        Self { rows, cols, data }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn as_slice(&self) -> &'a [f32] {
        self.data
    }

    pub fn row(&self, row: usize) -> &'a [f32] {
        assert!(row < self.rows);
        let start = row * self.cols;
        &self.data[start..start + self.cols]
    }
}
