# QuickGIFlick

> Select. Record. GIF.

QuickGIFlick is a Windows GIF recorder built on [ScreenDelta](https://github.com/ARTS-Night/ScreenDelta).
Its Cargo dependency is pinned to the validated ScreenDelta revision so a clean
clone and the Windows CI job build the same capture API.

## Current milestone

The current command-line pipeline captures the primary monitor at 15 FPS,
uses ScreenDelta `Full` / `Delta` / `Unchanged` updates, encodes an animated
GIF, and saves it to `%USERPROFILE%\\Videos\\QuickGIFlick`.

```powershell
cargo run --release
```

The in-memory pixel budget defaults to 32 MiB. Payload beyond that budget is
kept in an automatically removed temporary file so recording memory does not
grow with duration. Set `QUICKGIFFLICK_RECORDING_MEMORY_MB` only when testing a
different budget; it is intentionally not a user-facing setting yet. Set
`QUICKGIFFLICK_SECONDS` for automated capture duration tests.

The next UI milestone adds the `Win + Shift + G` selection overlay, recording
controls, review, and clipboard support. Those application behaviours remain
explicitly out of the current command-line proof of the capture/encode path.

## Validation

Run `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo clippy -- -D warnings`.
The recorder requires an interactive Windows desktop because it performs real
DXGI capture. `cargo run --release --example inspect_gif -- <path>` verifies a
saved GIF's decodability, frame count, and total GIF delay.
