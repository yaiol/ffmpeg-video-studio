# FFmpeg Video Studio

FFmpeg Video Studio is part of [yaiol Applications](https://apps.yaiol.com).

## Install

Download the latest installer from the [releases page](../../releases/latest).

## Build from source

Requires the Rust toolchain (stable) and, for the installer, the Dioxus CLI (`cargo install dioxus-cli`).

```bash
cargo run                                         # run in development
cd app && dx bundle --desktop --release --package-types nsis   # Windows installer → release/
```

## Architecture

A Cargo workspace of two crates:

- `engine/` — everything that is not the window: pure logic, no UI dependency.
- `app/` — the Dioxus desktop shell. The look comes from the shared stylesheet in `app/assets/`;
  strings come from `locales/<lang>.json`, embedded in the executable at build time.
