use super::cache::MatrixF64;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct WorldFeatures {
    pub base_f0: f64,
    pub f0: Vec<f64>,
    pub mgc: MatrixF64,
    pub bap: MatrixF64,
}
