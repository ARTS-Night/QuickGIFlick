# Windows controller verification — 2026-08-29

## Scope

The initial native controller owns only interaction. It registers the global
`Win + Shift + G` hotkey, creates its selection overlay across the virtual
desktop using Windows virtual-screen metrics, converts the selected screen
coordinates into `CaptureSource::Region`, and starts the existing recorder
only after an explicit confirmation.

This separation keeps QuickGIFlick-specific UI out of ScreenDelta and avoids a
second capture implementation. A selection that does not fit a single DXGI
output reaches ScreenDelta's explicit `Capture source must fit one active
monitor` error rather than producing a misleading recording.

## Windows MCP run

On 2026-08-29, Windows MCP reported one 1024×768, 96-DPI primary display.
The release executable was started, `Win + Shift + G` opened the topmost
`QuickGIFlick selection` window, and a drag from `(100,100)` to `(400,300)`
reached the record confirmation. Recording completed successfully.

The saved GIF was decoded by the repository's `inspect_gif` example:

| selected size | requested duration | decoded frames | decoded duration |
| --- | ---: | ---: | ---: |
| 300×200 | 3 seconds | 5 | 3.00 seconds |

This verifies global-hotkey dispatch, overlay input, region capture, GIF
creation, and GIF timing on an interactive Windows desktop. It does not claim
multi-monitor, review/trim, tray, clipboard, HUD exclusion, or browser
compatibility coverage; those require their own completed implementations and
tests.

`SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` is reserved for the future
HUD. Microsoft documents that value as suitable for recording controls on
Windows 10 version 2004 and newer, with compatibility behaviour on earlier
systems: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity>.
