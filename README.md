# QuickGIFlick

> Select. Record. GIF.

QuickGIFlick is a Windows GIF recorder built on [ScreenDelta](https://github.com/ARTS-Night/ScreenDelta).
Its Cargo dependency is pinned to the validated ScreenDelta revision so a clean
clone and the Windows CI job build the same capture API.

## Current milestone

The native Windows controller registers `Win + Shift + G`. It opens a virtual
desktop selection overlay, accepts a selected region, and asks for explicit
Record or Cancel confirmation. Recording runs on a worker and can be stopped
with `Win + Shift + G` (or the recording HUD). It uses ScreenDelta `Full` / `Delta` /
`Unchanged` updates, then opens Review with elapsed time and timeline-update
count. Choose Save or Discard; Save then selects Fast, Balanced, or Best GIF
quality and writes `%USERPROFILE%\\Videos\\QuickGIFlick\\QuickGIFlick_YYYY-MM-DD_HH-MM-SS.gif`.
Before quality selection, Review provides Start and End fields in seconds,
plus Full range and Cancel. The encoder reconstructs the canvas at the chosen
start timestamp.
The current ScreenDelta backend intentionally reports an error rather than
capturing an invalid region when a selection crosses monitor boundaries.

```powershell
cargo run --release
```

Release builds use the Windows GUI subsystem and do not open a console window.
Use the normal debug build for console diagnostics.

For repeatable benchmark capture rather than the UI, use
`$env:QUICKGIFFLICK_BENCH=1` and set `QUICKGIFFLICK_SECONDS`.

`QUICKGIFFLICK_QUALITY` accepts `fast`, `balanced` (default), or `best` for
the GIF palette quantizer. These presets do not alter capture resolution or
ScreenDelta's transport policy; see the measured trade-off in
[`docs/experiments/2026-08-29-quality-presets.md`](docs/experiments/2026-08-29-quality-presets.md).

After Save, choose **Yes** to copy the resulting GIF as a Windows file-drop
clipboard item. This is the natural format for pasting a GIF file into apps
that accept attachments; it is not a decoded bitmap clipboard representation.

The in-memory pixel budget defaults to 32 MiB. Payload beyond that budget is
kept in an automatically removed temporary file so recording memory does not
grow with duration. Set `QUICKGIFFLICK_RECORDING_MEMORY_MB` only when testing a
different budget; it is intentionally not a user-facing setting yet. Set
`QUICKGIFFLICK_SECONDS` for automated capture duration tests.

The controller also keeps a lightweight notification-area icon. Its compact
menu exposes Open, Start Capture, and Exit; the hotkey remains available while
it is resident.
The timeline now supports reconstructing the canvas at an arbitrary timestamp,
which is the correctness primitive used for Trim when a trim start falls after
a Delta update. The capture, bounded recording timeline, and encoder are kept
independent of the Win32 controller.

## Validation

Run `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo clippy -- -D warnings`.
The recorder requires an interactive Windows desktop because it performs real
DXGI capture. `cargo run --release --example inspect_gif -- <path>` verifies a
saved GIF's decodability, frame count, and total GIF delay.

Measured transport, recording, and Windows-controller evidence is consolidated
in [`docs/phase5-report.md`](docs/phase5-report.md).

For controlled encoder experiments only, `QUICKGIFFLICK_GIF_MODE=partial`
uses one bounding GIF rectangle per Delta update. It is not the default until
compatibility testing is complete; see
[`docs/experiments/2026-08-29-partial-gif.md`](docs/experiments/2026-08-29-partial-gif.md).
