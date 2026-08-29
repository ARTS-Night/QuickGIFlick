# QuickGIFlick

> Select. Record. GIF.

QuickGIFlick is a Windows GIF recorder built on [ScreenDelta](https://github.com/ARTS-Night/ScreenDelta).
Its Cargo dependency is pinned to the validated ScreenDelta revision so a clean
clone and the Windows CI job build the same capture API.

## Current milestone

The native Windows controller registers `Win + Shift + G`. It opens a virtual
desktop selection overlay, accepts a selected region, and asks for explicit
Record or Cancel confirmation. Recording uses ScreenDelta `Full` / `Delta` /
`Unchanged` updates and writes an animated GIF to
`%USERPROFILE%\\Videos\\QuickGIFlick\\QuickGIFlick_YYYY-MM-DD_HH-MM-SS.gif`.
The current ScreenDelta backend intentionally reports an error rather than
capturing an invalid region when a selection crosses monitor boundaries.

```powershell
cargo run --release
```

For repeatable benchmark capture rather than the UI, use
`$env:QUICKGIFFLICK_BENCH=1` and set `QUICKGIFFLICK_SECONDS`.

The in-memory pixel budget defaults to 32 MiB. Payload beyond that budget is
kept in an automatically removed temporary file so recording memory does not
grow with duration. Set `QUICKGIFFLICK_RECORDING_MEMORY_MB` only when testing a
different budget; it is intentionally not a user-facing setting yet. Set
`QUICKGIFFLICK_SECONDS` for automated capture duration tests.

The controller is intentionally small while the capture path is being measured:
review/trim, quality controls, clipboard and tray support are tracked as the
next product surface. The capture, bounded recording timeline, and encoder are
kept independent of the Win32 controller.

## Validation

Run `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo clippy -- -D warnings`.
The recorder requires an interactive Windows desktop because it performs real
DXGI capture. `cargo run --release --example inspect_gif -- <path>` verifies a
saved GIF's decodability, frame count, and total GIF delay.

For controlled encoder experiments only, `QUICKGIFFLICK_GIF_MODE=partial`
uses one bounding GIF rectangle per Delta update. It is not the default until
compatibility testing is complete; see
[`docs/experiments/2026-08-29-partial-gif.md`](docs/experiments/2026-08-29-partial-gif.md).
