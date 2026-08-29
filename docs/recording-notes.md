# Recording behaviour

QuickGIFlick samples ScreenDelta at the requested output rate (currently 15
FPS). A missing desktop update is not a dropped frame: it extends the previous
GIF frame's delay. This preserves recording duration on static screens and
avoids redundant GIF image data.

The first frame still waits for a real desktop update, which is the current
Desktop Duplication baseline limitation. The native selection UI and bounded
spill-backed long-recording buffer are implemented. ScreenDelta now supports
Original, Standard, and Hidden cursor transport; Monochrome and Masked Color
DXGI shapes remain outside the current implementation.

## Timing correction

The output pacer starts only after the first real frame arrives. Starting it
before that blocking operation caused catch-up sampling after an idle desktop,
which could distort frame intervals. GIF delay is derived from monotonic elapsed
time and carries fractional centiseconds forward, rather than using a fixed
seven-centisecond delay for every sample.

Windows MCP validation on 2026-08-28 moved the cursor once, then left the
desktop static. The recorder exited normally and saved a 1,404,087-byte GIF.
