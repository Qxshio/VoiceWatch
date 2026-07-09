use crate::settings::{self, Settings};
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, OnceLock};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DrawTextW, EndPaint, FillRect, GetStockObject, SetBkColor,
    SetBkMode, SetTextColor, DEFAULT_GUI_FONT, DT_LEFT, DT_SINGLELINE, DT_VCENTER, HBRUSH,
    PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetSystemMetrics, GetWindowLongPtrW, LoadCursorW, MessageBoxW, PostQuitMessage, RegisterClassW,
    SendMessageW, SetWindowLongPtrW, ShowWindow, TranslateMessage, BM_GETCHECK, BM_SETCHECK,
    BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CBS_DROPDOWNLIST, CBS_HASSTRINGS,
    CB_ADDSTRING, CB_ERR, CB_GETCURSEL, CB_SETCURSEL, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HMENU,
    IDC_ARROW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
    WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY, WM_PAINT,
    WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

const WINDOW_CLASS_NAME: &str = "VoiceWatchSettingsWindow";
const WINDOW_WIDTH: i32 = 460;
const WINDOW_HEIGHT: i32 = 480;
const BACKGROUND: u32 = 0x272523;
const TEXT: u32 = 0xeff0f2;
const MUTED_TEXT: u32 = 0x99958f;
const POLL_INTERVALS: [u64; 6] = [10, 15, 30, 60, 120, 300];

const ID_POLL_INTERVAL: i32 = 1001;
const ID_SMART_POLLING: i32 = 1002;
const ID_MIC_ACTIVE_PAUSE: i32 = 1003;
const ID_GAME_WINDOW_ONLY: i32 = 1004;
const ID_SHOW_OVERLAY: i32 = 1005;
const ID_PLAY_SOUND: i32 = 1006;
const ID_LAUNCH_STARTUP: i32 = 1007;
const ID_DEVELOPER_MODE: i32 = 1008;
const ID_SAVE: i32 = 1010;
const ID_CANCEL: i32 = 1011;
const ID_RESET: i32 = 1012;

const BST_UNCHECKED: usize = 0;
const BST_CHECKED: usize = 1;

static REGISTER_WINDOW_CLASS: Once = Once::new();
static SETTINGS_WINDOW_OPEN: AtomicBool = AtomicBool::new(false);
static BACKGROUND_BRUSH: OnceLock<usize> = OnceLock::new();

#[derive(Debug)]
struct Controls {
    poll_interval: HWND,
    smart_polling: HWND,
    mic_active_pause: HWND,
    game_window_only: HWND,
    show_overlay: HWND,
    play_sound: HWND,
    launch_startup: HWND,
    developer_mode: HWND,
}

#[derive(Debug, Clone, Copy)]
struct ControlRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Debug, Clone, Copy)]
struct ControlSpec<'a> {
    class_name: &'a str,
    text: &'a str,
    style: u32,
    rect: ControlRect,
    id: i32,
}

pub fn open() -> Result<()> {
    if SETTINGS_WINDOW_OPEN.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let thread = std::thread::Builder::new()
        .name("voice-watch-settings".into())
        .spawn(|| {
            if let Err(error) = run_window() {
                show_message(
                    "Voice Watch settings",
                    &format!("Could not open settings.\n\n{error:#}"),
                    MB_ICONERROR,
                );
            }
            SETTINGS_WINDOW_OPEN.store(false, Ordering::SeqCst);
        });

    if let Err(error) = thread {
        SETTINGS_WINDOW_OPEN.store(false, Ordering::SeqCst);
        return Err(error).context("failed to start settings window thread");
    }

    Ok(())
}

fn run_window() -> Result<()> {
    register_window_class();

    let title = wide("Voice Watch Settings");
    let class_name = wide(WINDOW_CLASS_NAME);
    let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let x = (unsafe { GetSystemMetrics(SM_CXSCREEN) } - WINDOW_WIDTH).max(0) / 2;
    let y = (unsafe { GetSystemMetrics(SM_CYSCREEN) } - WINDOW_HEIGHT).max(0) / 3;

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            x,
            y,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        )
    };

    if hwnd.is_null() {
        anyhow::bail!("failed to create settings window");
    }

    unsafe {
        ShowWindow(hwnd, SW_SHOW);
    }

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

fn register_window_class() {
    REGISTER_WINDOW_CLASS.call_once(|| {
        let class_name = wide(WINDOW_CLASS_NAME);
        let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(settings_window_proc),
            hInstance: hinstance,
            hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            hbrBackground: background_brush(),
            lpszClassName: class_name.as_ptr(),
            ..Default::default()
        };

        unsafe {
            RegisterClassW(&wnd_class);
        }
    });
}

unsafe extern "system" fn settings_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            match create_controls(hwnd) {
                Ok(controls) => {
                    let controls = Box::into_raw(Box::new(controls));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, controls as isize);
                }
                Err(error) => {
                    show_message(
                        "Voice Watch settings",
                        &format!("Could not create settings controls.\n\n{error:#}"),
                        MB_ICONERROR,
                    );
                    return -1;
                }
            }
            0
        }
        WM_COMMAND => match loword(wparam) {
            ID_SAVE => {
                match read_controls(hwnd).and_then(|settings| {
                    settings::save_settings(&settings)?;
                    settings::apply_launch_on_startup(settings.launch_on_startup)?;
                    Ok(())
                }) {
                    Ok(()) => {
                        show_message(
                            "Voice Watch settings",
                            "Settings saved.",
                            MB_ICONINFORMATION,
                        );
                        DestroyWindow(hwnd);
                    }
                    Err(error) => show_message(
                        "Voice Watch settings",
                        &format!("Could not save settings.\n\n{error:#}"),
                        MB_ICONERROR,
                    ),
                }
                0
            }
            ID_CANCEL => {
                DestroyWindow(hwnd);
                0
            }
            ID_RESET => {
                if let Some(controls) = controls(hwnd) {
                    populate_controls(controls, &Settings::default());
                }
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        },
        WM_PAINT => {
            paint_window(hwnd);
            0
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let hdc = wparam as _;
            SetBkColor(hdc, BACKGROUND);
            SetTextColor(hdc, TEXT);
            background_brush() as LRESULT
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Controls;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn create_controls(hwnd: HWND) -> Result<Controls> {
    let settings = settings::load_settings().unwrap_or_default();

    label(hwnd, "Polling", 24, 78, 150, 22)?;
    let poll_interval = control(
        hwnd,
        ControlSpec {
            class_name: "COMBOBOX",
            text: "",
            style: WS_TABSTOP | WS_VSCROLL | (CBS_DROPDOWNLIST as u32) | (CBS_HASSTRINGS as u32),
            rect: ControlRect {
                x: 248,
                y: 72,
                width: 170,
                height: 160,
            },
            id: ID_POLL_INTERVAL,
        },
    )?;

    for seconds in POLL_INTERVALS {
        let label = wide(&format!("{seconds} seconds"));
        SendMessageW(poll_interval, CB_ADDSTRING, 0, label.as_ptr() as LPARAM);
    }

    let controls = Controls {
        poll_interval,
        smart_polling: checkbox(hwnd, "Smart polling", 24, 116, ID_SMART_POLLING)?,
        mic_active_pause: checkbox(
            hwnd,
            "Pause while mic is active",
            24,
            150,
            ID_MIC_ACTIVE_PAUSE,
        )?,
        game_window_only: checkbox(
            hwnd,
            "Only check while Roblox is in-game",
            24,
            184,
            ID_GAME_WINDOW_ONLY,
        )?,
        show_overlay: checkbox(hwnd, "Show suspension HUD", 24, 218, ID_SHOW_OVERLAY)?,
        play_sound: checkbox(hwnd, "Play restore sound", 24, 252, ID_PLAY_SOUND)?,
        launch_startup: checkbox(hwnd, "Start with Windows", 24, 286, ID_LAUNCH_STARTUP)?,
        developer_mode: checkbox(hwnd, "Developer mode", 24, 320, ID_DEVELOPER_MODE)?,
    };

    button(hwnd, "Reset", rect(24, 360, 84, 28), ID_RESET, false)?;
    button(hwnd, "Cancel", rect(242, 360, 84, 28), ID_CANCEL, false)?;
    button(hwnd, "Save", rect(334, 360, 84, 28), ID_SAVE, true)?;

    populate_controls(&controls, &settings);
    Ok(controls)
}

unsafe fn label(hwnd: HWND, text: &str, x: i32, y: i32, width: i32, height: i32) -> Result<HWND> {
    control(
        hwnd,
        ControlSpec {
            class_name: "STATIC",
            text,
            style: 0,
            rect: ControlRect {
                x,
                y,
                width,
                height,
            },
            id: 0,
        },
    )
}

unsafe fn checkbox(hwnd: HWND, text: &str, x: i32, y: i32, id: i32) -> Result<HWND> {
    control(
        hwnd,
        ControlSpec {
            class_name: "BUTTON",
            text,
            style: WS_TABSTOP | (BS_AUTOCHECKBOX as u32),
            rect: rect(x, y, 360, 24),
            id,
        },
    )
}

unsafe fn button(
    hwnd: HWND,
    text: &str,
    rect: ControlRect,
    id: i32,
    primary: bool,
) -> Result<HWND> {
    control(
        hwnd,
        ControlSpec {
            class_name: "BUTTON",
            text,
            style: WS_TABSTOP
                | if primary {
                    BS_DEFPUSHBUTTON as u32
                } else {
                    BS_PUSHBUTTON as u32
                },
            rect,
            id,
        },
    )
}

unsafe fn control(hwnd: HWND, spec: ControlSpec<'_>) -> Result<HWND> {
    let class_name = wide(spec.class_name);
    let text = wide(spec.text);
    let child = CreateWindowExW(
        0,
        class_name.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE | spec.style,
        spec.rect.x,
        spec.rect.y,
        spec.rect.width,
        spec.rect.height,
        hwnd,
        control_id(spec.id),
        GetModuleHandleW(std::ptr::null()),
        std::ptr::null(),
    );

    if child.is_null() {
        anyhow::bail!("failed to create {} control", spec.class_name);
    }

    let font = GetStockObject(DEFAULT_GUI_FONT);
    SendMessageW(child, WM_SETFONT, font as WPARAM, 1);
    Ok(child)
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> ControlRect {
    ControlRect {
        x,
        y,
        width,
        height,
    }
}

fn populate_controls(controls: &Controls, settings: &Settings) {
    unsafe {
        let poll_index = POLL_INTERVALS
            .iter()
            .position(|seconds| *seconds == settings.poll_interval_seconds)
            .unwrap_or(0);
        SendMessageW(controls.poll_interval, CB_SETCURSEL, poll_index, 0);
        set_checked(controls.smart_polling, settings.smart_polling);
        set_checked(
            controls.mic_active_pause,
            settings.pause_polling_while_roblox_uses_microphone,
        );
        set_checked(
            controls.game_window_only,
            settings.only_poll_when_roblox_running,
        );
        set_checked(controls.show_overlay, settings.show_overlay);
        set_checked(controls.play_sound, settings.play_sound_on_restore);
        set_checked(controls.launch_startup, settings.launch_on_startup);
        set_checked(controls.developer_mode, settings.developer_mode);
    }
}

fn read_controls(hwnd: HWND) -> Result<Settings> {
    let controls = controls(hwnd).context("settings controls are not available")?;
    let selected_interval = unsafe { SendMessageW(controls.poll_interval, CB_GETCURSEL, 0, 0) };
    let poll_interval_seconds = if selected_interval == CB_ERR as isize {
        Settings::default().poll_interval_seconds
    } else {
        POLL_INTERVALS
            .get(selected_interval as usize)
            .copied()
            .unwrap_or_else(|| Settings::default().poll_interval_seconds)
    };

    Ok(Settings {
        poll_interval_seconds,
        only_poll_when_roblox_running: is_checked(controls.game_window_only),
        developer_mode: is_checked(controls.developer_mode),
        pause_polling_while_roblox_uses_microphone: is_checked(controls.mic_active_pause),
        smart_polling: is_checked(controls.smart_polling),
        show_overlay: is_checked(controls.show_overlay),
        play_sound_on_restore: is_checked(controls.play_sound),
        launch_on_startup: is_checked(controls.launch_startup),
    }
    .validate())
}

fn controls(hwnd: HWND) -> Option<&'static Controls> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Controls };
    (!ptr.is_null()).then(|| unsafe { &*ptr })
}

unsafe fn set_checked(hwnd: HWND, checked: bool) {
    SendMessageW(
        hwnd,
        BM_SETCHECK,
        if checked { BST_CHECKED } else { BST_UNCHECKED },
        0,
    );
}

fn is_checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) as usize == BST_CHECKED }
}

fn paint_window(hwnd: HWND) {
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client);
        FillRect(hdc, &client, background_brush());

        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, TEXT);
        DrawTextW(
            hdc,
            wide("Voice Watch").as_ptr(),
            -1,
            &mut RECT {
                left: 24,
                top: 20,
                right: 420,
                bottom: 46,
            },
            DT_LEFT | DT_SINGLELINE | DT_VCENTER,
        );

        SetTextColor(hdc, MUTED_TEXT);
        DrawTextW(
            hdc,
            wide("Settings").as_ptr(),
            -1,
            &mut RECT {
                left: 24,
                top: 45,
                right: 420,
                bottom: 66,
            },
            DT_LEFT | DT_SINGLELINE | DT_VCENTER,
        );

        EndPaint(hwnd, &paint);
    }
}

fn show_message(title: &str, body: &str, style: u32) {
    let title = wide(title);
    let body = wide(body);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | style,
        );
    }
}

fn background_brush() -> HBRUSH {
    *BACKGROUND_BRUSH.get_or_init(|| unsafe { CreateSolidBrush(BACKGROUND) as usize }) as HBRUSH
}

fn control_id(id: i32) -> HMENU {
    if id == 0 {
        std::ptr::null_mut()
    } else {
        id as usize as HMENU
    }
}

fn loword(value: WPARAM) -> i32 {
    (value & 0xffff) as i32
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
