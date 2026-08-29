# GIF quality preset experiment — 2026-08-29

## Question

Can QuickGIFlick offer an encoder quality control without coupling it to
ScreenDelta's capture or Delta policy?

## Method

`tools/run-controlled-gif-benchmark.ps1` ran the same two-second `small`
stimulus on the interactive Windows desktop. Each run produced both the safe
full-canvas GIF and the experimental partial GIF. `decoded_centiseconds` comes
from `inspect_gif`, so timing is verified separately from encoder speed.

| mode | quality | GIF bytes | encode wall ms | quantization ms | decoded time |
| --- | --- | ---: | ---: | ---: | ---: |
| full | fast | 770,898 | 603.924 | 474.947 | 2.01 s |
| full | balanced | 774,208 | 971.541 | 844.911 | 2.01 s |
| full | best | 826,434 | 7,807.261 | 7,667.632 | 2.01 s |
| partial | fast | 78,405 | 83.704 | 60.106 | 2.01 s |
| partial | balanced | 80,006 | 119.947 | 96.850 | 2.02 s |
| partial | best | 59,372 | 530.309 | 513.064 | 2.02 s |

## Decision

`balanced` remains the default because it preserves the existing encoder
behaviour and has substantially lower latency than `best`. `fast` and `best`
are opt-in environment settings for the upcoming UI selector. The data does
not justify changing the default GIF transport: partial GIF remains opt-in
until viewer compatibility coverage is complete.
