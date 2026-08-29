# Partial GIF experiment — 2026-08-29

## Method

The Phase 5 `small` controlled GDI stimulus ran on the 1366 x 768 interactive
Windows desktop. QuickGIFlick recorded for five seconds at 15 FPS. The existing
timeline and reusable canvas were unchanged. The experimental encoder writes
one GIF rectangle equal to each Delta update's bounding region with `Keep`
disposal; Full updates remain full-canvas frames.

| Mode | Decoded duration | GIF frames | Encode wall | Quantization | Output bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Full canvas | 5.01 s | 72 | 2,405.655 ms | 2,101.402 ms | 1,971,385 B |
| Partial bounding rect | 5.01 s | 73 | 366.800 ms | 313.183 ms | 226,210 B |

The two runs use the same deterministic workload and configuration, but their
capture start boundaries differ by one acquired update. The table is evidence
for the large partial-frame advantage on this workload, not a universal
percentage claim.

## Correctness

`partial_gif_keeps_unchanged_pixels` encodes an initial two-pixel canvas plus a
one-pixel Delta, decodes RGBA frames, composites their offsets, and verifies
both unchanged and changed pixels. The experimental five-second output also
decoded successfully with the expected GIF duration.

## Decision

Keep partial output opt-in through `QUICKGIFFLICK_GIF_MODE=partial`. Do not
make it the product default until static, typing, scroll, and cursor workloads
have decode/composition tests and at least one browser/viewer compatibility
check. Full canvas remains the safe default.
