//! Small native Windows controller.  It deliberately owns no capture details:
//! ScreenDelta remains the capture backend and the recorder runs only after a
//! user has chosen a screen-space region.

use std::{
    error::Error,
    sync::{Mutex, OnceLock},
};

use screendelta::{CaptureSource, Region};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT, Rectangle},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, ReleaseCapture, SetCapture,
                UnregisterHotKey,
            },
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                DispatchMessageW, GWLP_USERDATA, GetMessageW, GetSystemMetrics, GetWindowLongPtrW,
                IDC_CROSS, IDOK, IsWindow, LoadCursorW, MB_ICONERROR, MB_OK, MB_OKCANCEL, MSG,
                MessageBoxW, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
                SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, ShowWindow, TranslateMessage,
                WM_DESTROY, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT,
                WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            },
        },
    },
    core::{PCWSTR, w},
};

const HOTKEY_ID: i32 = 0x5147;
const OVERLAY_CLASS: PCWSTR = w!("QuickGIFlickSelectionOverlay");
static SELECTION: OnceLock<Mutex<Option<Region>>> = OnceLock::new();

struct OverlayState {
    origin: POINT,
    start: Option<POINT>,
    current: POINT,
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
    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            if message.message == WM_HOTKEY
                && message.wParam.0 == HOTKEY_ID as usize
                && let Some(region) = select_region()?
            {
                let choice = MessageBoxW(
                    None,
                    w!(
                        "Record this selected area now? Cancel returns to the hotkey. Recording length is controlled by QUICKGIFFLICK_SECONDS (default: 3 seconds)."
                    ),
                    w!("QuickGIFlick"),
                    MB_OKCANCEL,
                );
                if choice == IDOK {
                    match crate::run_recording(CaptureSource::Region(region)) {
                        Ok(path) => show_text(&format!("Saved {}", path.display()), MB_OK),
                        Err(error) => {
                            show_text(&format!("Recording failed: {error}"), MB_OK | MB_ICONERROR)
                        }
                    }
                }
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
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
