# Phase 5 report

## Scope and ownership

ScreenDelta owns DXGI capture, dirty-region transport, Full/Delta/Unchanged
updates, timestamps, and capture statistics. QuickGIFlick depends on that API
for its bounded recording timeline, canvas replay, GIF encoding, and native
Windows controller. There is no dependency in the reverse direction.

## Measured transport result

The controlled ScreenDelta matrix is recorded in the sibling repository at
[`ScreenDelta/docs/benchmarks/2026-08-29-controlled-phase5.md`](https://github.com/ARTS-Night/ScreenDelta/blob/main/docs/benchmarks/2026-08-29-controlled-phase5.md).
On the 1366×768 interactive Windows desktop, the cursor-only correction
changed 15-FPS transport from 146 Full updates to 1 Full and 142 Unchanged;
at 30 FPS it changed 246 Full updates to 1 Full and 267 Unchanged. This avoids
desktop readback for pointer metadata that contains no desktop pixels.

The retained policy sends Delta only for at most 32 regions covering less than
50% of the selected canvas. Window movement and full-screen motion use Full;
small, typing, and scroll stimuli commonly use Delta. The bounded staging cache
prevents one retained texture per update.

## Recording and timing result

The current release completed a 360×300, 60-second Windows MCP recording
through Selection → Record → Review → Full range → Balanced Save. The resulting
GIF decoded as 76 frames and 6,000 centiseconds (60.00 seconds). At 30 seconds
the process used 22,405,120 B working set and 5,414,912 B private memory; after
save those values were 19,263,488 B and 4,947,968 B. No
`quickgiflick-recording-*` temporary file remained after save.

Frames use monotonic timestamps. Unchanged updates extend GIF duration instead
of appending duplicate images, and fractional centisecond remainder is carried
forward so dropped or coalesced work does not make playback artificially fast.

## Cursor modes

The release cursor stimulus at 15 FPS for three seconds verified both current
modes. Default Original (`CursorCapture::Include`) produced 44 Delta updates
and 46 composited Color cursor updates; Full and Partial GIFs both decoded to
3.01 seconds. `QUICKGIFFLICK_CURSOR=hidden` produced 36–37 pointer-only
Unchanged updates with no composites, and both GIFs decoded to 3.00 seconds.
This preserves timestamp timing in either mode without forwarding a full frame
for pointer metadata alone.

Standard was then exercised with the same 15-FPS, three-second workload. Full
and Partial both decoded to 46 frames and 3.01 seconds; each observed 44 Delta
updates and 46 standard-cursor composites. The renderer matched the active
Windows standard cursor and would have fallen back to Arrow if unmatched.

The same Original cursor workload then ran for 30 seconds at 15 FPS. Full
encoded 451 frames and Partial encoded 452; both decoded to 30.01 seconds.
Full retained 30,299,156 B and Partial retained 22,132,332 B of timeline
payload, with 0 B spill in both runs. ScreenDelta reported 448 and 450 Delta
updates respectively, with 451 and 452 cursor composites. This is an actual
long-running cursor-composition regression, not an extrapolation from the
three-second run.

## Windows workflow verified

- `Win + Shift + G` invokes virtual-desktop selection and Record confirmation.
- Capture runs on a worker while the controller remains responsive.
- Stop leads to Review; trim accepts Start/End seconds, Full range, or Cancel.
- Fast, Balanced, and Best quality selection leads to the default Videos save
  directory.
- A release build has PE subsystem `GUI` and does not require a console.
- The notification-area icon was visible on the test host.
- GIF file clipboard creation was exercised as a `CF_HDROP` item.

## Validation

Both repositories have passing Rust formatting, check, test, and clippy gates.
QuickGIFlick's GitHub Actions job runs the same gates on `windows-latest`.

## Remaining limits

- The available host has one 96-DPI display; mixed-DPI, negative-coordinate,
  and multi-monitor interaction remain unproven.
- ScreenDelta composites supported DXGI Color cursor shapes for the default
  Original path, and `QUICKGIFFLICK_CURSOR=standard` renders matching Windows
  standard cursor shapes with Arrow fallback. `hidden` excludes cursor pixels.
  Monochrome and Masked Color DXGI shape support remains unimplemented.
- Partial GIF encoding is experimental; full-canvas output remains default
  pending compatibility coverage.
- The tray icon is verified, but its menu selection could not be asserted via
  this host's tray-overflow automation.
- Third-party attachment behavior for the file-drop clipboard item is not
  covered by this host.
