# Third-Party Notices

This project includes and/or links against third-party software. License texts are reproduced below (or provided under `./licenses/`) to satisfy redistribution requirements.

This file is an engineering compliance aid, not legal advice. Before publishing a binary release, regenerate or verify the full dependency license inventory with a tool such as `cargo about` or `cargo deny` against the exact `Cargo.lock` used for the release.

## WORLD (mmorise/World)

Organum includes a Rust-native implementation of the WORLD vocoder algorithm family. Several native modules intentionally map to WORLD concepts and source file names such as DIO, CheapTrick, D4C, and synthesis. The original WORLD project is BSD-style licensed, so source and binary redistributions should retain the WORLD copyright notice and disclaimer.

License: Modified BSD (BSD 3-Clause style)

Full text: `licenses/WORLD_BSD-3-Clause.txt`

## Rust crate dependencies

Organum is distributed as a Rust workspace and links Rust crates resolved by `Cargo.lock`. The root manifest currently declares dependencies including `anyhow`, `serde`, `serde_json`, `serde_yaml`, `hound`, `rubato`, `rayon`, `tracing`, `tracing-subscriber`, `zstd`, `getrandom`, and optional GPU-path crates `wgpu`, `pollster`, and `bytemuck`.

The dependency set is expected to be compatible with permissive open-source distribution, but the authoritative list is the lockfile for the release build. Release maintainers should generate the complete crate notice bundle from `Cargo.lock` and include it with binary artifacts when required by the dependency licenses.

Suggested release check:

```bash
cargo deny check licenses
cargo about generate about.hbs > THIRD_PARTY_CRATES.html
```

If the generated inventory differs from this summary, the generated inventory takes precedence for that release.
