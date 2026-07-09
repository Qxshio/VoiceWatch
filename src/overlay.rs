use crate::process;
use crate::rejoin::{open_rejoin_target, LastServer};
use anyhow::Result;
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, GetStockObject,
    InvalidateRect, LineTo, MoveToEx, RoundRect, SelectObject, SetBkMode, SetTextColor,
    DEFAULT_GUI_FONT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
    HGDIOBJ, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, LoadCursorW, MessageBoxW,
    RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, CS_DROPSHADOW,
    CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST, IDC_ARROW, IDOK, LWA_ALPHA, MB_ICONINFORMATION, MB_OK,
    MB_OKCANCEL, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA, WM_LBUTTONUP, WM_PAINT,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAction {
    Dismiss,
    RejoinLastServer,
}

#[derive(Debug, Clone)]
enum HudMode {
    Suspended { remaining: String },
    Restored,
}

#[derive(Debug, Clone)]
struct HudState {
    mode: HudMode,
    last_server: Option<LastServer>,
    restored_started_at: Option<Instant>,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            mode: HudMode::Suspended {
                remaining: "--".into(),
            },
            last_server: None,
            restored_started_at: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct SuspensionHud {
    hwnd: HWND,
}

const HUD_CLASS_NAME: &str = "VoiceWatchSuspensionHud";
const SUSPENDED_WIDTH: i32 = 286;
const RESTORED_WIDTH: i32 = 326;
const HUD_HEIGHT: i32 = 42;
const HUD_TOP_OFFSET: i32 = 12;
const RESTORE_ANIMATION_MS: u64 = 850;

static REGISTER_HUD_CLASS: Once = Once::new();
static HUD_SHARED: OnceLock<Mutex<HudState>> = OnceLock::new();

pub fn play_restore_sound() {
    unsafe {
        MessageBeep(MB_ICONINFORMATION);
    }
}

pub fn show_restored_overlay(last_server: Option<&LastServer>) -> Result<OverlayAction> {
    let can_rejoin = last_server.is_some_and(LastServer::is_actionable);
    let description = if can_rejoin {
        "Your VC suspension has expired.\n\nPress OK to rejoin your last known server, or Cancel to dismiss."
    } else {
        "Your VC suspension has expired.\n\nThe last server could not be identified."
    };

    let title = wide("Voice chat restored");
    let body = wide(description);
    let buttons = if can_rejoin { MB_OKCANCEL } else { MB_OK };
    let result = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            buttons | MB_ICONINFORMATION,
        )
    };

    let action = match result {
        IDOK if can_rejoin => OverlayAction::RejoinLastServer,
        _ => OverlayAction::Dismiss,
    };

    if let (OverlayAction::RejoinLastServer, Some(server)) = (action, last_server) {
        open_rejoin_target(server)?;
    }

    Ok(action)
}

impl SuspensionHud {
    pub fn show_suspended(&mut self, remaining: String) {
        *hud_shared().lock().expect("HUD state mutex poisoned") = HudState {
            mode: HudMode::Suspended { remaining },
            last_server: None,
            restored_started_at: None,
        };
        self.present_current_state();
    }

    pub fn show_restored(&mut self, last_server: Option<LastServer>) {
        let should_animate = {
            let mut state = hud_shared().lock().expect("HUD state mutex poisoned");
            let should_animate = !matches!(state.mode, HudMode::Restored);
            state.mode = HudMode::Restored;
            state.last_server = last_server;
            if should_animate || state.restored_started_at.is_none() {
                state.restored_started_at = Some(Instant::now());
            }
            should_animate
        };

        let hwnd = self.present_current_state();
        if should_animate {
            if let Some(hwnd) = hwnd {
                start_restore_animation(hwnd);
            }
        }
    }

    pub fn hide(&mut self) {
        if !self.hwnd.is_null() {
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    fn present_current_state(&mut self) -> Option<HWND> {
        let Some(bounds) = process::roblox_window_bounds() else {
            self.hide();
            return None;
        };

        let hwnd = self.ensure_window();
        if hwnd.is_null() {
            return None;
        }

        let size = hud_size();
        let x = bounds.left + ((bounds.width() - size.0) / 2).max(0);
        let y = bounds.top + HUD_TOP_OFFSET.min((bounds.height() / 3).max(0));

        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                x,
                y,
                size.0,
                size.1,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            InvalidateRect(hwnd, std::ptr::null(), 1);
        }
        Some(hwnd)
    }

    fn ensure_window(&mut self) -> HWND {
        if !self.hwnd.is_null() {
            return self.hwnd;
        }

        register_hud_class();
        let class_name = wide(HUD_CLASS_NAME);
        let title = wide("Voice Watch");
        let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let size = hud_size();

        self.hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                size.0,
                size.1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null(),
            )
        };

        if !self.hwnd.is_null() {
            unsafe {
                SetLayeredWindowAttributes(self.hwnd, 0, 244, LWA_ALPHA);
                ShowWindow(self.hwnd, SW_SHOWNA);
            }
        }

        self.hwnd
    }
}

impl Drop for SuspensionHud {
    fn drop(&mut self) {
        if !self.hwnd.is_null() {
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }
}

fn register_hud_class() {
    REGISTER_HUD_CLASS.call_once(|| {
        let class_name = wide(HUD_CLASS_NAME);
        let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
            lpfnWndProc: Some(hud_window_proc),
            hInstance: hinstance,
            hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            lpszClassName: class_name.as_ptr(),
            ..Default::default()
        };

        unsafe {
            RegisterClassW(&wnd_class);
        }
    });
}

unsafe extern "system" fn hud_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            paint_hud(hwnd);
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = lparam_point(lparam);
            if button_rect(hud_size()).is_some_and(|rect| point_in_rect(x, y, rect)) {
                let server = hud_shared()
                    .lock()
                    .expect("HUD state mutex poisoned")
                    .last_server
                    .clone();
                if let Some(server) = server {
                    let _ = open_rejoin_target(&server);
                    ShowWindow(hwnd, SW_HIDE);
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn hud_shared() -> &'static Mutex<HudState> {
    HUD_SHARED.get_or_init(|| Mutex::new(HudState::default()))
}

fn hud_size() -> (i32, i32) {
    match &hud_shared().lock().expect("HUD state mutex poisoned").mode {
        HudMode::Suspended { .. } => (SUSPENDED_WIDTH, HUD_HEIGHT),
        HudMode::Restored => (RESTORED_WIDTH, HUD_HEIGHT),
    }
}

fn paint_hud(hwnd: HWND) {
    let state = hud_shared()
        .lock()
        .expect("HUD state mutex poisoned")
        .clone();
    let restore_progress = if matches!(state.mode, HudMode::Restored) {
        restored_animation_progress(&state)
    } else {
        1.0
    };

    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client);

        let flash = if matches!(state.mode, HudMode::Restored) {
            (1.0 - restore_progress).max(0.0) * 0.38
        } else {
            0.0
        };
        let background = CreateSolidBrush(blend_color(rgb(35, 37, 39), rgb(34, 62, 47), flash));
        let border = CreatePen(
            PS_SOLID,
            1,
            blend_color(rgb(66, 69, 73), rgb(63, 203, 121), flash),
        );
        let old_brush = SelectObject(hdc, background as HGDIOBJ);
        let old_pen = SelectObject(hdc, border as HGDIOBJ);
        RoundRect(
            hdc,
            client.left,
            client.top,
            client.right,
            client.bottom,
            8,
            8,
        );
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(border as HGDIOBJ);
        DeleteObject(background as HGDIOBJ);

        SelectObject(hdc, GetStockObject(DEFAULT_GUI_FONT));
        SetBkMode(hdc, TRANSPARENT as i32);

        match state.mode {
            HudMode::Suspended { remaining } => paint_suspended(hdc, remaining),
            HudMode::Restored => paint_restored(hdc, state.last_server.as_ref(), restore_progress),
        }

        EndPaint(hwnd, &paint);
    }
}

fn paint_suspended(hdc: windows_sys::Win32::Graphics::Gdi::HDC, remaining: String) {
    let badge = RECT {
        left: 10,
        top: 8,
        right: 104,
        bottom: 32,
    };

    unsafe {
        let badge_brush = CreateSolidBrush(rgb(190, 66, 66));
        let badge_pen = CreatePen(PS_SOLID, 1, rgb(205, 82, 82));
        let old_brush = SelectObject(hdc, badge_brush as HGDIOBJ);
        let old_pen = SelectObject(hdc, badge_pen as HGDIOBJ);
        RoundRect(hdc, badge.left, badge.top, badge.right, badge.bottom, 7, 7);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(badge_pen as HGDIOBJ);
        DeleteObject(badge_brush as HGDIOBJ);
    }

    draw_text(
        hdc,
        "Suspended",
        badge,
        rgb(255, 255, 255),
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    draw_text(
        hdc,
        &remaining,
        RECT {
            left: 116,
            top: 0,
            right: SUSPENDED_WIDTH - 12,
            bottom: HUD_HEIGHT,
        },
        rgb(232, 235, 239),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
    );
}

fn paint_restored(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    last_server: Option<&LastServer>,
    progress: f32,
) {
    let eased = ease_out_cubic(progress);
    let badge = RECT {
        left: 10,
        top: 8,
        right: 36,
        bottom: 32,
    };
    let badge_flash = (1.0 - progress).max(0.0) * 0.55;

    unsafe {
        let badge_brush = CreateSolidBrush(blend_color(
            rgb(42, 184, 120),
            rgb(96, 236, 151),
            badge_flash,
        ));
        let badge_pen = CreatePen(PS_SOLID, 1, rgb(83, 218, 138));
        let old_brush = SelectObject(hdc, badge_brush as HGDIOBJ);
        let old_pen = SelectObject(hdc, badge_pen as HGDIOBJ);
        RoundRect(hdc, badge.left, badge.top, badge.right, badge.bottom, 8, 8);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(badge_pen as HGDIOBJ);
        DeleteObject(badge_brush as HGDIOBJ);

        let check_pen = CreatePen(PS_SOLID, 2, rgb(255, 255, 255));
        let old_pen = SelectObject(hdc, check_pen as HGDIOBJ);
        MoveToEx(hdc, 17, 21, std::ptr::null_mut());
        LineTo(hdc, 22, 26);
        LineTo(hdc, 31, 16);
        SelectObject(hdc, old_pen);
        DeleteObject(check_pen as HGDIOBJ);
    }

    draw_text(
        hdc,
        "Voice restored",
        RECT {
            left: 46,
            top: 0,
            right: 214,
            bottom: HUD_HEIGHT,
        },
        rgb(232, 235, 239),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    let Some(rect) = button_rect((RESTORED_WIDTH, HUD_HEIGHT)) else {
        return;
    };
    let can_rejoin = last_server.is_some_and(LastServer::is_actionable);
    let fill = if can_rejoin {
        rgb(242, 243, 243)
    } else {
        rgb(86, 89, 94)
    };
    let text = if can_rejoin { "Rejoin" } else { "No server" };
    let text_color = if can_rejoin {
        rgb(35, 37, 39)
    } else {
        rgb(190, 196, 202)
    };

    unsafe {
        let brush = CreateSolidBrush(fill);
        let pen = CreatePen(PS_SOLID, 1, rgb(71, 74, 79));
        let old_brush = SelectObject(hdc, brush as HGDIOBJ);
        let old_pen = SelectObject(hdc, pen as HGDIOBJ);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 7, 7);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(pen as HGDIOBJ);
        DeleteObject(brush as HGDIOBJ);
    }

    draw_text(
        hdc,
        text,
        rect,
        text_color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    let sweep_width = ((RESTORED_WIDTH - 20) as f32 * eased).round() as i32;
    if sweep_width > 0 && progress < 1.0 {
        unsafe {
            let brush = CreateSolidBrush(rgb(42, 184, 120));
            let pen = CreatePen(PS_SOLID, 1, rgb(42, 184, 120));
            let old_brush = SelectObject(hdc, brush as HGDIOBJ);
            let old_pen = SelectObject(hdc, pen as HGDIOBJ);
            RoundRect(
                hdc,
                10,
                HUD_HEIGHT - 4,
                10 + sweep_width,
                HUD_HEIGHT - 2,
                2,
                2,
            );
            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_brush);
            DeleteObject(pen as HGDIOBJ);
            DeleteObject(brush as HGDIOBJ);
        }
    }
}

fn draw_text(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &str,
    mut rect: RECT,
    color: u32,
    format: u32,
) {
    let text = wide(text);
    unsafe {
        SetTextColor(hdc, color);
        DrawTextW(hdc, text.as_ptr(), -1, &mut rect, format);
    }
}

fn button_rect(size: (i32, i32)) -> Option<RECT> {
    (size.0 >= RESTORED_WIDTH).then_some(RECT {
        left: size.0 - 96,
        top: 8,
        right: size.0 - 12,
        bottom: size.1 - 8,
    })
}

fn point_in_rect(x: i32, y: i32, rect: RECT) -> bool {
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

fn lparam_point(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam & 0xffff) as i16 as i32;
    let y = ((lparam >> 16) & 0xffff) as i16 as i32;
    (x, y)
}

fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}

fn blend_color(from: u32, to: u32, amount: f32) -> u32 {
    let amount = amount.clamp(0.0, 1.0);
    let blend = |shift: u32| {
        let start = ((from >> shift) & 0xff_u32) as f32;
        let end = ((to >> shift) & 0xff_u32) as f32;
        (start + ((end - start) * amount)).round() as u8
    };

    rgb(blend(0), blend(8), blend(16))
}

fn restored_animation_progress(state: &HudState) -> f32 {
    let Some(started_at) = state.restored_started_at else {
        return 1.0;
    };

    (started_at.elapsed().as_millis() as f32 / RESTORE_ANIMATION_MS as f32).clamp(0.0, 1.0)
}

fn ease_out_cubic(value: f32) -> f32 {
    1.0 - (1.0 - value).powi(3)
}

fn start_restore_animation(hwnd: HWND) {
    let hwnd_value = hwnd as isize;
    std::thread::spawn(move || {
        let started_at = Instant::now();
        let duration = Duration::from_millis(RESTORE_ANIMATION_MS);
        while started_at.elapsed() <= duration {
            unsafe {
                InvalidateRect(hwnd_value as HWND, std::ptr::null(), 1);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        unsafe {
            InvalidateRect(hwnd_value as HWND, std::ptr::null(), 1);
        }
    });
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
