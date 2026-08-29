//! Small native Windows controller.  It deliberately owns no capture details:
//! ScreenDelta remains the capture backend and the recorder runs only after a
//! user has chosen a screen-space region.

use std::{
    error::Error,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

use screendelta::{CaptureSource, Region};
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            BLACK_BRUSH, BeginPaint, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DrawTextW, EndPaint,
            GetStockObject, InvalidateRect, PAINTSTRUCT, Rectangle,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, ReleaseCapture, SetCapture,
                UnregisterHotKey,
            },
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW, GetSystemMetrics,
                GetWindowLongPtrW, IDC_CROSS, IDOK, IsWindow, KillTimer, LWA_ALPHA, LoadCursorW,
                MB_ICONERROR, MB_OK, MB_OKCANCEL, MSG, MessageBoxW, RegisterClassW,
                SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
                SW_SHOW, SetLayeredWindowAttributes, SetTimer, SetWindowDisplayAffinity,
                ShowWindow, TranslateMessage, WDA_EXCLUDEFROMCAPTURE, WM_DESTROY, WM_HOTKEY,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_TIMER, WNDCLASSW,
                WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            },
        },
    },
    core::{PCWSTR, w},
};

const HOTKEY_ID: i32 = 0x5147;
const RECORDING_TIMER_ID: usize = 0x5147;
const OVERLAY_CLASS: PCWSTR = w!("QuickGIFlickSelectionOverlay");
const HUD_CLASS: PCWSTR = w!("QuickGIFlickRecordingHud");
static SELECTION: OnceLock<Mutex<Option<Region>>> = OnceLock::new();
static HUD_STOP: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();

struct OverlayState {
    origin: POINT,
    start: Option<POINT>,
    current: POINT,
}

struct ActiveRecording {
    stop: Arc<AtomicBool>,
    completed: Receiver<Result<std::path::PathBuf, String>>,
    hud: HWND,
}

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    unsafe {
        RegisterHotKey(
            None,
            HOTKEY_ID,
            MOD_WIN | MOD_SHIFT | MOD_NOREPEAT,
            u32::from(b'G'),
        )?;
    }
    let result = message_loop();
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID);
    }
    result
}

fn message_loop() -> Result<(), Box<dyn Error>> {
    let mut active = None;
    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            if message.message == WM_TIMER {
                finish_recording(&mut active);
            }
            if message.message == WM_HOTKEY && message.wParam.0 == HOTKEY_ID as usize {
                if let Some(recording) = &active {
                    recording.stop.store(true, Ordering::Relaxed);
                } else if let Some(region) = select_region()? {
                    let choice = MessageBoxW(
                        None,
                        w!(
                            "Record this selected area now? Cancel returns to the hotkey. Recording length is controlled by QUICKGIFFLICK_SECONDS (default: 3 seconds)."
                        ),
                        w!("QuickGIFlick"),
                        MB_OKCANCEL,
                    );
                    if choice == IDOK {
                        active = Some(start_recording(region)?);
                    }
                }
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

fn start_recording(region: Region) -> Result<ActiveRecording, Box<dyn Error>> {
    let stop = Arc::new(AtomicBool::new(false));
    *HUD_STOP
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("HUD stop lock") = Some(stop.clone());
    let (sender, completed) = mpsc::channel();
    let worker_stop = stop.clone();
    thread::spawn(move || {
        let result = crate::run_recording_until(CaptureSource::Region(region), Some(&worker_stop))
            .map_err(|error| error.to_string());
        let _ = sender.send(result);
    });
    let hud = show_recording_hud()?;
    unsafe {
        let _ = SetTimer(None, RECORDING_TIMER_ID, 100, None);
    }
    Ok(ActiveRecording {
        stop,
        completed,
        hud,
    })
}

fn finish_recording(active: &mut Option<ActiveRecording>) {
    let Some(recording) = active.as_ref() else {
        return;
    };
    let Ok(result) = recording.completed.try_recv() else {
        return;
    };
    unsafe {
        let _ = DestroyWindow(recording.hud);
        let _ = KillTimer(None, RECORDING_TIMER_ID);
    }
    *HUD_STOP
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("HUD stop lock") = None;
    *active = None;
    match result {
        Ok(path) => show_text(&format!("Saved {}", path.display()), MB_OK),
        Err(error) => show_text(&format!("Recording failed: {error}"), MB_OK | MB_ICONERROR),
    }
}

fn show_recording_hud() -> Result<HWND, Box<dyn Error>> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: HUD_CLASS,
            lpfnWndProc: Some(hud_proc),
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            HUD_CLASS,
            w!("QuickGIFlick recording"),
            WS_POPUP,
            24,
            24,
            310,
            56,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 220, LWA_ALPHA);
        // Best effort: Windows 10 version 2004+ excludes this top-level HUD
        // from supported capture paths; failure leaves recording functional.
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        let _ = ShowWindow(hwnd, SW_SHOW);
        Ok(hwnd)
    }
}

fn select_region() -> Result<Option<Region>, Box<dyn Error>> {
    let slot = SELECTION.get_or_init(|| Mutex::new(None));
    *slot.lock().expect("selection lock") = None;
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let cursor = LoadCursorW(None, IDC_CROSS)?;
        let class = WNDCLASSW {
            hCursor: cursor,
            hInstance: instance.into(),
            lpszClassName: OVERLAY_CLASS,
            lpfnWndProc: Some(overlay_proc),
            style: CS_HREDRAW | CS_VREDRAW,
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&class);
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let state = Box::new(OverlayState {
            origin: POINT { x, y },
            start: None,
            current: POINT::default(),
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            OVERLAY_CLASS,
            w!("QuickGIFlick selection"),
            WS_POPUP,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        use windows::Win32::UI::WindowsAndMessaging::{GWLP_USERDATA, SetWindowLongPtrW};
        let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        // Global alpha provides an unobtrusive dimmer while retaining normal
        // desktop visibility during selection.
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 145, LWA_ALPHA);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
            if !IsWindow(Some(hwnd)).as_bool() {
                break;
            }
        }
    }
    Ok(slot.lock().expect("selection lock").take())
}

unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    message: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_LBUTTONDOWN => {
            let state = unsafe { state(hwnd) };
            let point = point_from_lparam(lparam);
            state.start = Some(point);
            state.current = point;
            let _ = unsafe { SetCapture(hwnd) };
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let state = unsafe { state(hwnd) };
            if state.start.is_some() {
                state.current = point_from_lparam(lparam);
                let _ = unsafe { InvalidateRect(Some(hwnd), None, true) };
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let state = unsafe { state(hwnd) };
            if let Some(start) = state.start {
                let end = point_from_lparam(lparam);
                let left = start.x.min(end.x) + state.origin.x;
                let top = start.y.min(end.y) + state.origin.y;
                let width = (start.x - end.x).unsigned_abs();
                let height = (start.y - end.y).unsigned_abs();
                if let Some(region) = Region::new(left, top, width, height) {
                    *SELECTION
                        .get_or_init(|| Mutex::new(None))
                        .lock()
                        .expect("selection lock") = Some(region);
                }
            }
            let _ = unsafe { ReleaseCapture() };
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut paint) };
            if let Some(start) = unsafe { state(hwnd) }.start {
                let end = unsafe { state(hwnd) }.current;
                let _ = unsafe {
                    Rectangle(
                        dc,
                        start.x.min(end.x),
                        start.y.min(end.y),
                        start.x.max(end.x),
                        start.y.max(end.y),
                    )
                };
            }
            let _ = unsafe { EndPaint(hwnd, &paint) };
            LRESULT(0)
        }
        WM_DESTROY => {
            let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayState };
            if !pointer.is_null() {
                unsafe {
                    drop(Box::from_raw(pointer));
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, _wparam, lparam) },
    }
}

unsafe extern "system" fn hud_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_LBUTTONDOWN => {
            if let Some(stop) = HUD_STOP
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("HUD stop lock")
                .as_ref()
            {
                stop.store(true, Ordering::Relaxed);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut paint) };
            let mut rect = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut rect) };
            let mut text: Vec<u16> = "● REC  Click or Win+Shift+G to stop"
                .encode_utf16()
                .collect();
            let _ = unsafe {
                DrawTextW(
                    dc,
                    &mut text,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                )
            };
            let _ = unsafe { EndPaint(hwnd, &paint) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

unsafe fn state(hwnd: HWND) -> &'static mut OverlayState {
    unsafe { &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayState) }
}

fn point_from_lparam(value: LPARAM) -> POINT {
    POINT {
        x: (value.0 as u32 & 0xffff) as i16 as i32,
        y: ((value.0 as u32 >> 16) & 0xffff) as i16 as i32,
    }
}

fn show_text(text: &str, flags: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE) {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let _ = MessageBoxW(None, PCWSTR(wide.as_ptr()), w!("QuickGIFlick"), flags);
    }
}
