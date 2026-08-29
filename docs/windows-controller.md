# Windows controller verification — 2026-08-29

## Scope

The initial native controller owns only interaction. It registers the global
`Win + Shift + G` hotkey, creates its selection overlay across the virtual
desktop using Windows virtual-screen metrics, converts the selected screen
coordinates into `CaptureSource::Region`, and starts the existing recorder
only after an explicit confirmation.

When capture stops, the worker returns the bounded Delta timeline to the UI
instead of encoding immediately. Review shows the elapsed duration and update
count, then offers Save or Discard. The retained
timeline is the foundation for trim and quality controls without blocking
capture on GIF encoding.

After Save, a compact native quality choice selects Fast, Balanced, or Best
quantization. It changes only GIF encoding work; capture and the Delta timeline
are unchanged.

During a drag, the overlay uses a Win32 window region to cut the selected
rectangle out of the translucent dimmer. The selected desktop pixels therefore
remain at normal brightness; the overlay keeps mouse capture until the drag
ends. This avoids treating a drawn white rectangle as a substitute for an
actual selection preview.

The controller sets `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` before it
creates any Win32 window. This keeps the selection overlay and ScreenDelta's
desktop `Region` in physical pixels rather than DPI-virtualized coordinates.

This separation keeps QuickGIFlick-specific UI out of ScreenDelta and avoids a
second capture implementation. A selection that does not fit a single DXGI
output reaches ScreenDelta's explicit `Capture source must fit one active
monitor` error rather than producing a misleading recording.

## Windows MCP run

On 2026-08-29, Windows MCP reported one 1024×768, 96-DPI primary display.
The release executable was started, `Win + Shift + G` opened the topmost
`QuickGIFlick selection` window, and a drag from `(100,100)` to `(400,300)`
reached the record confirmation. Recording completed successfully.

The current release was also exercised through capture, Review, Save, and the
Fast/Balanced/Best quality chooser; the chooser was visible and interactive.

The current native Trim dialog exposes Start and End (seconds), Save range,
Full range, and Cancel. Windows MCP opened it from a three-second recording
with `0.00` and `3.00` populated, and confirmed that both fields receive text.
This host's input automation appends text instead of replacing a selected
native edit value, so its final custom-range save is covered by the Rust
range-validation and timeline-reconstruction tests rather than claimed as a
completed end-to-end UI assertion.

The saved GIF was decoded by the repository's `inspect_gif` example:

| selected size | requested duration | decoded frames | decoded duration |
| --- | ---: | ---: | ---: |
| 300×200 | 3 seconds | 5 | 3.00 seconds |

The controller now starts capture on a worker thread, leaving the Windows
message loop active. A topmost `● REC` HUD exposes Stop by click or by the same
`Win + Shift + G` hotkey, and requests `WDA_EXCLUDEFROMCAPTURE` best-effort.
In a second Windows MCP run, hotkey Stop produced a decoded GIF of 5 frames and
0.53 seconds, proving that the recorded end timestamp follows stop rather than
the three-second development cap. The exclusion flag means screenshot-driven
automation cannot be used to prove the HUD click hit-test; the hotkey is the
verified stop path.

Save then offers **Yes / No** for Copy. In a Windows MCP run, **Yes** produced
the `GIF file copied to clipboard` success dialog. Windows MCP's clipboard read
reported non-text data, as expected for the `CF_HDROP` file-drop format used by
the implementation. This proves construction and ownership transfer of the
file clipboard item; attachment behaviour in individual third-party apps still
needs app-specific compatibility coverage.

This verifies global-hotkey dispatch, overlay input, non-blocking region capture,
Stop timing, GIF creation, GIF timing, and file clipboard construction on an
interactive Windows desktop. The available host has a single 96-DPI display,
so it proves the V2 API initializes and this physical-pixel path works there.
It does not prove mixed-DPI or negative-coordinate multi-monitor operation,
Tray, HUD exclusion in a captured GIF, or third-party clipboard compatibility;
those require their own completed implementations and tests.

The HUD requests `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` best effort.
Microsoft documents that value as suitable for recording controls on
Windows 10 version 2004 and newer, with compatibility behaviour on earlier
systems: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity>.
