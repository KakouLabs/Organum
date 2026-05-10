# WORLD

Native Rust implementation of WORLD vocoder algorithms.

## Project Structure

- `src/common.rs`: Common data structures (e.g., `MatrixF32`) used across the crate.
- `src/native/`: Main Rust-native implementation of WORLD.
    - `analysis.rs`: F0 estimation (DIO, Harvest) and Spectral analysis (CheapTrick).
    - `synthesis.rs`: Waveform synthesis.
    - `d4c.rs`: Aperiodicity estimation.
    - `codec.rs`: Spectral and aperiodicity parameter compression.
- `src/reference/`: Reference implementations (often using `f64` for higher precision/comparison).
    - `d4c.rs`: Reference implementation of D4C.

## Usage

Most users should use the `native` module:

```rust
use world::native::{analyze, synthesis};
```

Core types are also available at the crate root:

```rust
use world::MatrixF32;
```
