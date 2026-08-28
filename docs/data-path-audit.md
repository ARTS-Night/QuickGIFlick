# Data path audit — 2026-08-28

## Observed path

QuickGIFlick is in-process with ScreenDelta. It receives an owned `CpuFrame`
through `into_readback`, keeps changed frames in `Vec<RecordedFrame>`, and only
at Stop converts each owned BGRA vector in place to RGBA for `gif::Frame`.

```text
CpuFrame ownership move -> Recording Vec -> in-place BGRA/RGBA swap
-> gif quantization/encoding
```

There is no channel, Canvas, resize, or CPU frame clone on this path. The
recording vector is unbounded, which is the observed long-recording bottleneck.

## Measured baseline

Windows MCP 30-second recording, 1366 x 768 desktop: peak working set at ten
seconds was 349,179,904 bytes; resulting GIF was 17,192,497 bytes. This is the
baseline for comparing a timeline/streaming transport.

## Candidate transports

1. Current full CPU timeline: simple but memory grows with changed frames.
2. Delta timeline: initial full state plus timestamped regions; candidate for
   small motion, needs canvas-correctness experiment.
3. Streaming encoder with bounded timeline: bounds memory, but must preserve
   trim/review semantics; deferred until the timeline experiment.
