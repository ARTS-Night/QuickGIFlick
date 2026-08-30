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
- The tray host was found by its registered Windows class and received the
  native `Open` command; Windows MCP then exposed the QuickGIFlick window.
  `Start Capture` was likewise dispatched through the tray `WM_COMMAND` path
  and opened the full-screen selection window. The overflow menu itself still
  cannot be enumerated by this host's accessibility tree. Finally, dispatching
  the native `Exit` command terminated the release process within 500 ms.
- GIF file clipboard creation was exercised as a `CF_HDROP` item.

## Validation

Both repositories have passing Rust formatting, check, test, and clippy gates.
QuickGIFlick's GitHub Actions job runs the same gates on `windows-latest`.

### 2026-08-30 lightweight ownership pass

Timeline reconstruction now borrows stored-frame metadata and the spill file
as disjoint fields. This removes the former temporary `StoredFrame` clones;
for an in-memory Full frame, reconstruction now creates only the required
output pixel buffer instead of cloning that buffer twice. Raw spill fallback
also moves the original pixels instead of cloning them. The native controller
now runs its 100 ms polling timer only while recording or encoding, and treats
a disconnected worker as an error instead of waiting forever.

An actual 1920×1080, 30-FPS, five-second capture with an 8 MiB recording
budget completed with 14 GIF frames and decoded to exactly 500 centiseconds.
Peak working set was 44,699,648 B, peak private memory was 39,501,824 B, and
the 2,778,048 B logical spill occupied 37,759 B. A separate three-second idle
sample consumed 0.000 ms of process CPU and held an 8,339,456 B working set.

The final native-UI regression found that the layered recording HUD retained
its initial `00:00` paint even though capture and stop polling continued. The
timer is now owned by the HUD window, forces a synchronous Win32 redraw, and
updates the accessible window title once per second. Windows automation
verified both the visible text and title at `00:03`, then stopped recording and
reached Review normally. The progress window uses the same window-owned timer.

The former text-only Review prompt is now a native animated preview. Playback
uses monotonic recording timestamps, applies only due Full/Delta updates to one
reused canvas, and loops without building a full-frame preview cache. A
19-second Windows recording with 27 updates advanced visibly and through its
accessible title from `00:07` to `00:15`, then wrapped to `00:06`; Discard
closed it normally. Static intervals repaint only when the displayed second
changes, while visual updates repaint when their timestamp is reached.

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
