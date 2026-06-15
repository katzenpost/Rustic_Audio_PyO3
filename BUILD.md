# Building the Rust Audio Module for KatzenQT

This crate produces the PyO3-backed shared object consumed by `katzenqt`.
katzenqt` now loads the module from its package directory at `katzenqt/src/katzenqt/rustic_audio_tool.so`.

## Prerequisites

On Debian/Ubuntu:

```bash
sudo apt-get install -y build-essential pkg-config libasound2-dev
```

You also need a current Rust toolchain:

```bash
cargo --version
```

`rustup` is the expected install path on Linux.

## Standalone Cargo Build

From this directory:

```bash
cargo build
```

The PyO3 cdylib artifact will be written to:

```text
target/debug/librustic_audio_tool.so
```

## Integrated KatzenQT Build

From `katzenqt/`:

```bash
make setup-uv
make rust-audio
```

That does two things:

1. builds this crate via `cargo build --manifest-path ../Rust_Audio_Lib_mod/Cargo.toml`
2. copies `target/debug/librustic_audio_tool.so` to:

```text
katzenqt/src/katzenqt/rustic_audio_tool.so
```

At runtime, `katzenqt/src/katzenqt/audio_ptt.py` first looks for the shared
object next to itself in the package directory, then falls back to legacy
site-packages lookup only if needed.

## Validation

After `make rust-audio`, you can verify the source-tree runtime path from
`katzenqt/`:

```bash
PYTHONPATH=src .venv/bin/python -c "from katzenqt.audio_ptt import _load_backend_module; print(_load_backend_module())"
```

## Local Vendor Note

This workspace vendors `opus-rs` under `vendor/opus-rs` because upstream 0.1.22
used unstable `usize::is_multiple_of` on stable Rust. Keep that local patch in
place unless the dependency is updated to a fixed upstream release.
