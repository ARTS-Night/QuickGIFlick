# Production roadmap

This is the release plan for QuickGIFlick and its ScreenDelta dependency. The
first production target is a reliable Windows recorder for a region contained
within one monitor. Cross-monitor capture and experimental GIF optimizations are
not release blockers for that target.

## Release candidate blockers (P0)

- Recover from DXGI Access Lost, display changes, sleep/resume, and monitor
  disconnect without terminating the process.
- Encode to a temporary file and rename only after a successful GIF write.
- Fall back safely when a DXGI Monochrome or Masked Color cursor shape cannot be
  rasterized; never silently omit the cursor in Original mode.
- Exercise 10, 30, and 60 minute recordings on Windows 10 and 11, including
  100%, 125%, and 150% DPI and a negative-coordinate monitor.
- Verify the GitHub EXE artifact on a clean Windows machine and record the exact
  ScreenDelta revision used by the build.

## Reliability and usability (P1)

- Verify Clipboard file-drop behavior in Explorer, Edge/Chrome, and Discord.
- Keep Review responsive when a spilled timeline payload takes a slow read and
  leave a recoverable error state inside the Review window.
- Test disk-full, access-denied save, clipboard failure, and encoder failure;
  each must explain the failure and return to a safe UI state.
- Add keyboard names and focus behavior for Review, Trim, HUD, and progress
  controls.

## Deferred until measured

- Cross-monitor recording and mixed-DPI selection.
- Partial GIF as the default output.
- GPU delta shaders, SIMD comparison, and a new encoder dependency.
- Installer, auto-update, telemetry, and non-Windows backends.

## Release gates

The release candidate must preserve wall-clock GIF duration, keep memory from
growing without bound during endurance tests, avoid silent frame loss, and pass
`cargo fmt --check`, `cargo check`, `cargo test`, and `cargo clippy -- -D warnings`
in both repositories.
