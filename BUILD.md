# Building the Rust Audio Module for KatzenQT

This crate produces the PyO3-backed shared object consumed by `katzenqt`.
katzenqt` loads the module from its package directory at `katzenqt/src/katzenqt/audio/rustic_audio_tool.so`.

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

On Linux, Cargo always prefixes cdylib output with `lib`. KatzenQT loads
`rustic_audio_tool.so` from `katzenqt/src/katzenqt/audio/`; use `make rust-audio`
from `katzenqt/` to rename and install.

## Integrated KatzenQT Build

From `katzenqt/`:

```bash
make setup-uv
make rust-audio
```

That does two things:

1. builds this crate via `cargo build`
2. installs `target/debug/librustic_audio_tool.so` as:

```text
katzenqt/src/katzenqt/audio/rustic_audio_tool.so
```

A copy is also written to `target/debug/rustic_audio_tool.so` in this crate.

At runtime, `katzenqt/src/katzenqt/audio_ptt.py` looks for
`src/katzenqt/audio/rustic_audio_tool.so`, then falls back to legacy
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
