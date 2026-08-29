//! Small native Windows controller.  It deliberately owns no capture details:
//! ScreenDelta remains the capture backend and the recorder runs only after a
//! user has chosen a screen-space region.

use std::os::windows::ffi::OsStrExt;
use std::{
    error::Error,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

use crate::recording::Recording;
use screendelta::{CaptureSource, Region};
use windows::{
    Win32::{
        Foundation::{COLORREF, GlobalFree, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            BLACK_BRUSH, BeginPaint, CombineRgn, CreateRectRgn, DT_CENTER, DT_SINGLELINE,
            DT_VCENTER, DeleteObject, DrawTextW, EndPaint, GetStockObject, InvalidateRect,
            PAINTSTRUCT, RGN_DIFF, Rectangle, SetWindowRgn,
        },
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            LibraryLoader::GetModuleHandleW,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_HDROP,
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Input::KeyboardAndMouse::{
                MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, ReleaseCapture, SetCapture,
                UnregisterHotKey,
            },
            Shell::DROPFILES,
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW, GetSystemMetrics,
                GetWindowLongPtrW, IDC_CROSS, IDCANCEL, IDNO, IDOK, IDYES, IsWindow, KillTimer,
                LWA_ALPHA, LoadCursorW, MB_ICONERROR, MB_OK, MB_OKCANCEL, MB_YESNO, MB_YESNOCANCEL,
                MSG, MessageBoxW, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
                SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, SetLayeredWindowAttributes,
                SetTimer, SetWindowDisplayAffinity, ShowWindow, TranslateMessage,
                WDA_EXCLUDEFROMCAPTURE, WM_DESTROY, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP,
                WM_MOUSEMOVE, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
                WS_EX_TOPMOST, WS_POPUP,
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
    completed: Receiver<Result<Recording, String>>,
    hud: HWND,
}

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    // ScreenDelta regions use desktop physical pixels; make the overlay use
    // that same coordinate space even when it crosses monitors with different
    // scale factors. This must happen before any UI is created.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)? };
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
        let result =
            crate::capture_recording_until(CaptureSource::Region(region), Some(&worker_stop))
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
        Ok(mut recording) => review_recording(&mut recording),
        Err(error) => show_text(&format!("Recording failed: {error}"), MB_OK | MB_ICONERROR),
    }
}

fn review_recording(recording: &mut Recording) {
    let message = format!(
        "Review: {:.2}s, {} timeline updates.\n\nSave a balanced GIF now?",
        recording.end().as_secs_f64(),
        recording.update_len(),
    );
    if show_question(&message) != IDYES {
        return;
    }
    let (start, end) = choose_trim_range(recording.end());
    let quality = choose_quality();
    let output = match crate::output_path() {
        Ok(path) => path,
        Err(error) => {
            return show_text(&format!("Save path failed: {error}"), MB_OK | MB_ICONERROR);
        }
    };
    match crate::encode_recording_range(
        recording,
        &output,
        crate::GifMode::Full,
        quality,
        start,
        end,
    ) {
        Ok(_) => offer_copy(&output),
        Err(error) => show_text(
            &format!("GIF encoding failed: {error}"),
            MB_OK | MB_ICONERROR,
        ),
    }
}

fn choose_trim_range(end: std::time::Duration) -> (std::time::Duration, std::time::Duration) {
    let midpoint = end / 2;
    match show_choice("Trim range:\n\nYes = first half\nNo = last half\nCancel = full recording") {
        IDYES => (std::time::Duration::ZERO, midpoint),
        IDNO => (midpoint, end),
        _ => (std::time::Duration::ZERO, end),
    }
}

fn choose_quality() -> crate::GifQuality {
    let choice = show_choice(
        "GIF quality:\n\nYes = Fast (quickest)\nNo = Balanced\nCancel = Best (smallest / slowest)",
    );
    match choice {
        IDYES => crate::GifQuality::Fast,
        IDCANCEL => crate::GifQuality::Best,
        _ => crate::GifQuality::Balanced,
    }
}

fn offer_copy(path: &std::path::Path) {
    let message = format!("Saved {}\n\nCopy GIF file to clipboard?", path.display());
    let answer = show_question(&message);
    if answer == IDYES {
        match copy_file_to_clipboard(path) {
            Ok(()) => show_text("GIF file copied to clipboard.", MB_OK),
            Err(error) => show_text(
                &format!("Clipboard copy failed: {error}"),
                MB_OK | MB_ICONERROR,
            ),
        }
    }
}

fn copy_file_to_clipboard(path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let mut name: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let bytes = std::mem::size_of::<DROPFILES>() + std::mem::size_of_val(name.as_slice());
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes)?;
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err("GlobalLock failed".into());
        }
        let header = pointer.cast::<DROPFILES>();
        *header = DROPFILES {
            pFiles: std::mem::size_of::<DROPFILES>() as u32,
            fWide: true.into(),
            ..Default::default()
        };
        std::ptr::copy_nonoverlapping(
            name.as_mut_ptr().cast::<u8>(),
            pointer.cast::<u8>().add(std::mem::size_of::<DROPFILES>()),
            std::mem::size_of_val(name.as_slice()),
        );
        let _ = GlobalUnlock(memory);
        OpenClipboard(None)?;
        if let Err(error) = EmptyClipboard()
            .and_then(|_| SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(memory.0))))
        {
            let _ = CloseClipboard();
            let _ = GlobalFree(Some(memory));
            return Err(error.into());
        }
        CloseClipboard()?;
    }
    Ok(())
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
                unsafe { update_overlay_cutout(hwnd, state) };
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

/// Make only the unselected area part of the layered overlay.  Unlike drawing
/// a pale rectangle over a uniformly translucent window, this leaves the
/// selected desktop pixels unmodified while capture remains held by the
/// overlay window.
unsafe fn update_overlay_cutout(hwnd: HWND, state: &OverlayState) {
    let Some(start) = state.start else {
        return;
    };
    let mut client = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
        return;
    }
    let outer = unsafe { CreateRectRgn(client.left, client.top, client.right, client.bottom) };
    let selected = unsafe {
        CreateRectRgn(
            start.x.min(state.current.x),
            start.y.min(state.current.y),
            start.x.max(state.current.x),
            start.y.max(state.current.y),
        )
    };
    let _ = unsafe { CombineRgn(Some(outer), Some(outer), Some(selected), RGN_DIFF) };
    // SetWindowRgn takes ownership of `outer`; `selected` remains ours.
    let _ = unsafe { SetWindowRgn(hwnd, Some(outer), true) };
    let _ = unsafe { DeleteObject(selected.into()) };
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

fn show_question(text: &str) -> windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe { MessageBoxW(None, PCWSTR(wide.as_ptr()), w!("QuickGIFlick"), MB_YESNO) }
}

fn show_choice(text: &str) -> windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(wide.as_ptr()),
            w!("QuickGIFlick quality"),
            MB_YESNOCANCEL,
        )
    }
}
