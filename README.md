# QuickGIFlick

> Select. Record. GIF.

QuickGIFlick is a Windows GIF recorder built on [ScreenDelta](https://github.com/ARTS-Night/ScreenDelta).

## Current milestone

The first working pipeline captures the primary monitor for three seconds at
15 FPS, encodes an animated GIF, and saves it to `%USERPROFILE%\\Videos\\QuickGIFlick`.

```powershell
cargo run --release
```

The next UI milestone adds the `Win + Shift + G` selection overlay, recording
controls, review, and clipboard support. Those application behaviours remain
explicitly out of the current command-line proof of the capture/encode path.

## Validation

Run `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo clippy -- -D warnings`.
The recorder requires an interactive Windows desktop because it performs real
DXGI capture.
