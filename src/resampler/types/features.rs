use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct WorldFeatures {
    pub base_f0: f64,
    pub f0: Vec<f64>,
    pub mgc: Vec<Vec<f64>>,
    pub bap: Vec<Vec<f64>>,
}
