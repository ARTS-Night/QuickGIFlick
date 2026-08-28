# Data path audit — 2026-08-28

## Observed path

QuickGIFlick is in-process with ScreenDelta. It receives one initial Full
canvas, then timestamped Full / Delta / Unchanged updates. A reusable canvas
applies Delta pixels at their capture-local coordinates during GIF generation.
Unchanged updates only advance the timeline end time.

```text
Full/Delta ownership move -> bounded Recording payload store -> reusable Canvas
-> reusable BGRA/RGBA output buffer -> gif quantization/encoding
```

The store has a 32 MiB default resident-pixel budget. Payload that does not fit
is appended to one temporary file and read back only for final reconstruction;
metadata remains in memory. The temporary file is removed on normal `Recording`
drop. There is deliberately no unbounded pixel queue.

## Measured baseline

Windows MCP 30-second recording, 1366 x 768 desktop: peak working set at ten
seconds was 349,179,904 bytes; resulting GIF was 17,192,497 bytes. This is the
baseline for comparing a timeline/streaming transport.

A repeat run measured 94 stored full frames with 394,457,088 bytes of payload
capacity at Stop; the 10-second working set was 290,414,592 bytes. This confirms
that recording-frame payload, rather than ScreenDelta's reusable staging
texture or GIF output, is the dominant growth source.

## Adopted transport

1. Initial Full CPU canvas, then dirty-region Delta payloads where ScreenDelta
   selects them; large or uncertain updates remain Full.
2. Timeline timestamps preserve actual capture time through Unchanged periods
   and GIF centisecond remainder accumulation.
3. Bounded resident storage with raw-payload spill preserves future trim/replay
   capability without retaining a raw full-frame array in RAM.

Final GIF encoding still writes full-canvas GIF frames. Partial-GIF encoding is
not claimed here: correctness and browser compatibility have not yet been
validated for it.
