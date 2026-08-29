# 0002: Trim from the Delta-native timeline

## Decision

Do not retain a separate full-frame copy for every possible trim start.
`Recording::canvas_at(timestamp)` begins with the initial full canvas and
applies stored Full/Delta updates through the requested timestamp. GIF range
encoding then writes that reconstructed canvas as the first trim frame and
continues with later updates.

## Correctness test

`trim_starts_from_canvas_reconstructed_at_delta_time` records a Delta at 10ms,
starts a trim at 15ms, decodes the resulting GIF, and asserts that its first
frame includes the Delta pixel. This prevents a common trim bug where a Delta
start is rendered against the initial canvas.

## Cost

Reconstruction reads only the normal RecordingStore payload and reuses the
existing Canvas buffer. Keyframes are deliberately not added until a measured
preview/seek workload demonstrates they are necessary.
