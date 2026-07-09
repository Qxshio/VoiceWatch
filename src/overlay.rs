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
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowRect, LoadCursorW,
    MessageBoxW, RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow,
    CS_DROPSHADOW, CS_HREDRAW, CS_VREDRAW, HTCAPTION, HWND_TOPMOST, IDC_ARROW, IDOK, LWA_ALPHA,
    MB_ICONINFORMATION, MB_OK, MB_OKCANCEL, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA,
    WM_LBUTTONUP, WM_NCHITTEST, WM_PAINT, WM_WINDOWPOSCHANGED, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HudButton {
    Hide,
    Rejoin,
}

#[derive(Debug, Clone)]
struct HudState {
    mode: HudMode,
    last_server: Option<LastServer>,
    restored_started_at: Option<Instant>,
    hidden: bool,
    manual_offset: Option<(i32, i32)>,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            mode: HudMode::Suspended {
                remaining: "--".into(),
            },
            last_server: None,
            restored_started_at: None,
            hidden: false,
            manual_offset: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct SuspensionHud {
    hwnd: HWND,
}

const HUD_CLASS_NAME: &str = "VoiceWatchSuspensionHud";
const SUSPENDED_WIDTH: i32 = 328;
const RESTORED_WIDTH: i32 = 420;
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
    let can_rejoin = last_server.is_some_and(LastServer::can_rejoin_exact);
    let description = if can_rejoin {
        "Your VC suspension has expired.\n\nPress OK to rejoin your last known server, or Cancel to dismiss."
    } else {
        "Your VC suspension has expired.\n\nThe exact server could not be identified."
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
        let hidden = {
            let mut state = hud_shared().lock().expect("HUD state mutex poisoned");
            let hidden = matches!(state.mode, HudMode::Suspended { .. }) && state.hidden;
            let manual_offset = state.manual_offset;
            *state = HudState {
                mode: HudMode::Suspended { remaining },
                last_server: None,
                restored_started_at: None,
                hidden,
                manual_offset,
            };
            hidden
        };
        if hidden {
            self.hide();
            return;
        }
        self.present_current_state();
    }

    pub fn show_restored(&mut self, last_server: Option<LastServer>) {
        let (should_animate, hidden) = {
            let mut state = hud_shared().lock().expect("HUD state mutex poisoned");
            let should_animate = !matches!(state.mode, HudMode::Restored);
            if should_animate {
                state.hidden = false;
            }
            state.mode = HudMode::Restored;
            state.last_server = last_server;
            if should_animate || state.restored_started_at.is_none() {
                state.restored_started_at = Some(Instant::now());
            }
            (should_animate, state.hidden)
        };

        if hidden {
            self.hide();
            return;
        }

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
        if hud_shared()
            .lock()
            .expect("HUD state mutex poisoned")
            .hidden
        {
            self.hide();
            return None;
        }

        let Some(bounds) = process::roblox_window_bounds() else {
            self.hide();
            return None;
        };

        let hwnd = self.ensure_window();
        if hwnd.is_null() {
            return None;
        }

        let size = hud_size();
        let (default_x, default_y) = default_hud_position(bounds, size);
        let (offset_x, offset_y) = hud_shared()
            .lock()
            .expect("HUD state mutex poisoned")
            .manual_offset
            .unwrap_or((0, 0));
        let x = default_x.saturating_add(offset_x);
        let y = default_y.saturating_add(offset_y);

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
        WM_NCHITTEST => {
            if let Some((x, y)) = screen_lparam_to_client(hwnd, lparam) {
                if point_in_rect(x, y, drag_handle_rect()) {
                    return HTCAPTION as LRESULT;
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_WINDOWPOSCHANGED => {
            remember_manual_offset(hwnd);
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_LBUTTONUP => {
            let (x, y) = lparam_point(lparam);
            let action = {
                let state = hud_shared().lock().expect("HUD state mutex poisoned");
                button_at_point(&state, x, y)
            };

            match action {
                Some(HudButton::Hide) => {
                    hud_shared()
                        .lock()
                        .expect("HUD state mutex poisoned")
                        .hidden = true;
                    ShowWindow(hwnd, SW_HIDE);
                }
                Some(HudButton::Rejoin) => {
                    let server = hud_shared()
                        .lock()
                        .expect("HUD state mutex poisoned")
                        .last_server
                        .clone();
                    let Some(server) = server else {
                        return 0;
                    };
                    let _ = open_rejoin_target(&server);
                    ShowWindow(hwnd, SW_HIDE);
                }
                None => {}
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
    paint_drag_handle(hdc);

    let badge = RECT {
        left: 32,
        top: 8,
        right: 126,
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
            left: 138,
            top: 0,
            right: suspended_hide_button_rect().left - 8,
            bottom: HUD_HEIGHT,
        },
        rgb(232, 235, 239),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
    );

    paint_button(hdc, suspended_hide_button_rect(), "Hide", true, false);
}

fn paint_restored(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    last_server: Option<&LastServer>,
    progress: f32,
) {
    paint_drag_handle(hdc);

    let eased = ease_out_cubic(progress);
    let badge = RECT {
        left: 32,
        top: 8,
        right: 58,
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
        MoveToEx(hdc, 39, 21, std::ptr::null_mut());
        LineTo(hdc, 44, 26);
        LineTo(hdc, 53, 16);
        SelectObject(hdc, old_pen);
        DeleteObject(check_pen as HGDIOBJ);
    }

    draw_text(
        hdc,
        "Voice restored",
        RECT {
            left: 68,
            top: 0,
            right: restored_hide_button_rect().left - 8,
            bottom: HUD_HEIGHT,
        },
        rgb(232, 235, 239),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    let can_rejoin = last_server.is_some_and(LastServer::can_rejoin_exact);
    paint_button(hdc, restored_hide_button_rect(), "Hide", true, false);
    paint_button(
        hdc,
        restored_rejoin_button_rect(),
        if can_rejoin { "Rejoin" } else { "No server" },
        can_rejoin,
        true,
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
                32,
                HUD_HEIGHT - 4,
                32 + sweep_width,
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

fn paint_drag_handle(hdc: windows_sys::Win32::Graphics::Gdi::HDC) {
    let rect = drag_handle_rect();
    unsafe {
        let brush = CreateSolidBrush(rgb(141, 146, 153));
        let old_brush = SelectObject(hdc, brush as HGDIOBJ);
        for row in 0..3 {
            for column in 0..2 {
                let left = rect.left + 3 + (column * 7);
                let top = rect.top + 4 + (row * 7);
                RoundRect(hdc, left, top, left + 4, top + 4, 3, 3);
            }
        }
        SelectObject(hdc, old_brush);
        DeleteObject(brush as HGDIOBJ);
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

fn paint_button(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    rect: RECT,
    text: &str,
    enabled: bool,
    primary: bool,
) {
    let fill = match (enabled, primary) {
        (true, true) => rgb(242, 243, 243),
        (true, false) => rgb(56, 59, 64),
        (false, _) => rgb(86, 89, 94),
    };
    let text_color = match (enabled, primary) {
        (true, true) => rgb(35, 37, 39),
        (true, false) => rgb(232, 235, 239),
        (false, _) => rgb(190, 196, 202),
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
}

fn button_at_point(state: &HudState, x: i32, y: i32) -> Option<HudButton> {
    if point_in_rect(x, y, hide_button_rect(&state.mode)) {
        return Some(HudButton::Hide);
    }

    if matches!(state.mode, HudMode::Restored)
        && state
            .last_server
            .as_ref()
            .is_some_and(LastServer::can_rejoin_exact)
        && point_in_rect(x, y, restored_rejoin_button_rect())
    {
        return Some(HudButton::Rejoin);
    }

    None
}

fn drag_handle_rect() -> RECT {
    RECT {
        left: 8,
        top: 8,
        right: 24,
        bottom: HUD_HEIGHT - 8,
    }
}

fn hide_button_rect(mode: &HudMode) -> RECT {
    match mode {
        HudMode::Suspended { .. } => suspended_hide_button_rect(),
        HudMode::Restored => restored_hide_button_rect(),
    }
}

fn suspended_hide_button_rect() -> RECT {
    RECT {
        left: SUSPENDED_WIDTH - 72,
        top: 8,
        right: SUSPENDED_WIDTH - 12,
        bottom: HUD_HEIGHT - 8,
    }
}

fn restored_hide_button_rect() -> RECT {
    let rejoin = restored_rejoin_button_rect();
    RECT {
        left: rejoin.left - 68,
        top: 8,
        right: rejoin.left - 8,
        bottom: HUD_HEIGHT - 8,
    }
}

fn restored_rejoin_button_rect() -> RECT {
    RECT {
        left: RESTORED_WIDTH - 96,
        top: 8,
        right: RESTORED_WIDTH - 12,
        bottom: HUD_HEIGHT - 8,
    }
}

fn point_in_rect(x: i32, y: i32, rect: RECT) -> bool {
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

fn lparam_point(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam & 0xffff) as i16 as i32;
    let y = ((lparam >> 16) & 0xffff) as i16 as i32;
    (x, y)
}

fn window_rect(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    (unsafe { GetWindowRect(hwnd, &mut rect) } != 0).then_some(rect)
}

fn screen_lparam_to_client(hwnd: HWND, lparam: LPARAM) -> Option<(i32, i32)> {
    let (screen_x, screen_y) = lparam_point(lparam);
    let rect = window_rect(hwnd)?;
    Some((
        screen_x.saturating_sub(rect.left),
        screen_y.saturating_sub(rect.top),
    ))
}

fn remember_manual_offset(hwnd: HWND) {
    let (Some(window), Some(bounds)) = (window_rect(hwnd), process::roblox_window_bounds()) else {
        return;
    };
    let (default_x, default_y) = default_hud_position(bounds, hud_size());
    let manual_offset = (
        window.left.saturating_sub(default_x),
        window.top.saturating_sub(default_y),
    );
    hud_shared()
        .lock()
        .expect("HUD state mutex poisoned")
        .manual_offset = Some(manual_offset);
}

fn default_hud_position(bounds: process::WindowBounds, size: (i32, i32)) -> (i32, i32) {
    (
        bounds.left + ((bounds.width() - size.0) / 2).max(0),
        bounds.top + HUD_TOP_OFFSET.min((bounds.height() / 3).max(0)),
    )
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
