use crate::native_host_registration::BrowserTarget;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{FALSE, HWND, LPARAM, TRUE};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VK_CONTROL, VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

#[derive(Debug, Clone)]
pub struct BrowserInstall {
    pub target: BrowserTarget,
}

struct BrowserDefinition {
    target: BrowserTarget,
    key: &'static str,
    exe_name: &'static str,
    launch_url: &'static str,
    window_title: &'static str,
    relative_paths: &'static [&'static str],
}

const DEFINITIONS: &[BrowserDefinition] = &[
    BrowserDefinition {
        target: BrowserTarget::Brave,
        key: "brave",
        exe_name: "brave.exe",
        launch_url: "chrome://extensions/",
        window_title: "Brave",
        relative_paths: &[
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Programs\BraveSoftware\Brave-Browser\Application\brave.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Chrome,
        key: "chrome",
        exe_name: "chrome.exe",
        launch_url: "chrome://extensions/",
        window_title: "Google Chrome",
        relative_paths: &[
            r"Google\Chrome\Application\chrome.exe",
            r"Programs\Google\Chrome\Application\chrome.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Edge,
        key: "edge",
        exe_name: "msedge.exe",
        launch_url: "edge://extensions/",
        window_title: "Microsoft Edge",
        relative_paths: &[
            r"Microsoft\Edge\Application\msedge.exe",
            r"Programs\Microsoft\Edge\Application\msedge.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Vivaldi,
        key: "vivaldi",
        exe_name: "vivaldi.exe",
        launch_url: "chrome://extensions/",
        window_title: "Vivaldi",
        relative_paths: &[
            r"Vivaldi\Application\vivaldi.exe",
            r"Programs\Vivaldi\Application\vivaldi.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Opera,
        key: "opera",
        exe_name: "opera.exe",
        launch_url: "chrome://extensions/",
        window_title: "Opera",
        relative_paths: &[
            r"Programs\Opera\opera.exe",
            r"Opera\opera.exe",
            r"Opera GX\opera.exe",
            r"Programs\Opera GX\opera.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Chromium,
        key: "chromium",
        exe_name: "chrome.exe",
        launch_url: "chrome://extensions/",
        window_title: "Chromium",
        relative_paths: &[
            r"Chromium\Application\chrome.exe",
            r"Programs\Chromium\Application\chrome.exe",
        ],
    },
];

pub fn installed_browsers() -> Vec<BrowserInstall> {
    DEFINITIONS
        .iter()
        .filter_map(|definition| {
            find_browser_exe(definition).map(|_| BrowserInstall {
                target: definition.target,
            })
        })
        .collect()
}

pub fn setup_query(is_connected: bool) -> String {
    let installed = installed_browsers();
    let browsers = installed
        .iter()
        .filter_map(|install| browser_key(install.target))
        .collect::<Vec<_>>()
        .join(",");
    let preferred = installed
        .first()
        .and_then(|install| browser_key(install.target))
        .unwrap_or_default();
    format!(
        "browsers={}&preferred={}&connected={}",
        encode_query_value(&browsers),
        encode_query_value(preferred),
        if is_connected { "1" } else { "0" }
    )
}

pub fn open_extensions_page(browser: BrowserTarget) -> Result<()> {
    let target = match browser {
        BrowserTarget::All => installed_browsers()
            .first()
            .map(|install| install.target)
            .context("no supported Chromium browser was found")?,
        specific => specific,
    };
    let definition = definition_for_target(target)
        .with_context(|| format!("unsupported browser target: {target}"))?;
    let exe_path = find_browser_exe(definition)
        .with_context(|| format!("{} is not installed", definition_display_name(target)))?;

    open_extensions_page_in_browser(&exe_path, definition)
        .with_context(|| format!("failed to open {}", definition_display_name(target)))?;
    Ok(())
}

fn open_extensions_page_in_browser(exe_path: &Path, definition: &BrowserDefinition) -> Result<()> {
    Command::new(exe_path)
        .args(["--new-window", "about:blank"])
        .spawn()
        .context("failed to start browser")?;

    sleep(Duration::from_millis(700));

    if let Some(hwnd) = wait_for_browser_window(definition.window_title, Duration::from_secs(5)) {
        focus_window(hwnd);
        sleep(Duration::from_millis(250));
        send_ctrl_l().context("failed to focus browser address bar")?;
        sleep(Duration::from_millis(100));
        send_text(definition.launch_url).context("failed to type extensions page address")?;
        sleep(Duration::from_millis(100));
        send_virtual_key(VK_RETURN).context("failed to open extensions page")?;
        return Ok(());
    }

    Command::new(exe_path)
        .arg(definition.launch_url)
        .spawn()
        .context("failed to start browser fallback")?;
    Ok(())
}

fn wait_for_browser_window(title_fragment: &str, timeout: Duration) -> Option<HWND> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if let Some(hwnd) = find_window_by_title(title_fragment) {
            return Some(hwnd);
        }

        sleep(Duration::from_millis(150));
    }

    None
}

fn find_window_by_title(title_fragment: &str) -> Option<HWND> {
    let mut search = WindowSearch {
        title_fragment: title_fragment.to_lowercase(),
        setup_hwnd: std::ptr::null_mut(),
        fallback_hwnd: std::ptr::null_mut(),
    };

    unsafe {
        EnumWindows(
            Some(enum_window_for_title),
            (&mut search as *mut WindowSearch).cast::<std::ffi::c_void>() as LPARAM,
        );
    }

    if !search.setup_hwnd.is_null() {
        Some(search.setup_hwnd)
    } else {
        (!search.fallback_hwnd.is_null()).then_some(search.fallback_hwnd)
    }
}

struct WindowSearch {
    title_fragment: String,
    setup_hwnd: HWND,
    fallback_hwnd: HWND,
}

unsafe extern "system" fn enum_window_for_title(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam as *mut WindowSearch);

    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }

    let text_length = GetWindowTextLengthW(hwnd);
    if text_length <= 0 {
        return 1;
    }

    let mut buffer = vec![0; text_length as usize + 1];
    let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    if copied <= 0 {
        return 1;
    }

    let title = String::from_utf16_lossy(&buffer[..copied as usize]).to_lowercase();
    if title.contains(&search.title_fragment) {
        if search.fallback_hwnd.is_null() {
            search.fallback_hwnd = hwnd;
        }

        if is_setup_browser_title(&title) {
            search.setup_hwnd = hwnd;
            return 0;
        }
    }

    1
}

fn is_setup_browser_title(title: &str) -> bool {
    title.contains("about:blank") || title.contains("new tab") || title.contains("extensions")
}

fn focus_window(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        BringWindowToTop(hwnd);

        let current_thread = GetCurrentThreadId();
        let foreground = GetForegroundWindow();
        let foreground_thread = if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, std::ptr::null_mut())
        };
        let target_thread = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());

        if foreground_thread != 0 {
            AttachThreadInput(current_thread, foreground_thread, TRUE);
        }
        if target_thread != 0 {
            AttachThreadInput(current_thread, target_thread, TRUE);
        }

        SetForegroundWindow(hwnd);
        BringWindowToTop(hwnd);

        if target_thread != 0 {
            AttachThreadInput(current_thread, target_thread, FALSE);
        }
        if foreground_thread != 0 {
            AttachThreadInput(current_thread, foreground_thread, FALSE);
        }
    }
}

fn send_ctrl_l() -> Result<()> {
    send_inputs(&[
        virtual_key_input(VK_CONTROL, false),
        virtual_key_input(b'L' as u16, false),
        virtual_key_input(b'L' as u16, true),
        virtual_key_input(VK_CONTROL, true),
    ])
}

fn send_virtual_key(key: u16) -> Result<()> {
    send_inputs(&[virtual_key_input(key, false), virtual_key_input(key, true)])
}

fn send_text(text: &str) -> Result<()> {
    let inputs = text
        .encode_utf16()
        .flat_map(|unit| [unicode_input(unit, false), unicode_input(unit, true)])
        .collect::<Vec<_>>();

    send_inputs(&inputs)
}

fn send_inputs(inputs: &[INPUT]) -> Result<()> {
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };

    (sent == inputs.len() as u32)
        .then_some(())
        .context("Windows did not accept keyboard input")
}

fn virtual_key_input(key: u16, key_up: bool) -> INPUT {
    keyboard_input(key, 0, if key_up { KEYEVENTF_KEYUP } else { 0 })
}

fn unicode_input(unit: u16, key_up: bool) -> INPUT {
    keyboard_input(
        0,
        unit,
        KEYEVENTF_UNICODE | if key_up { KEYEVENTF_KEYUP } else { 0 },
    )
}

fn keyboard_input(key: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn browser_from_protocol_url(url: &str) -> Result<BrowserTarget> {
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .context("setup link is missing query parameters")?;
    let browser = query_value(query, "browser").unwrap_or_else(|| "all".into());
    BrowserTarget::parse(&browser)
}

pub fn browser_key(target: BrowserTarget) -> Option<&'static str> {
    definition_for_target(target).map(|definition| definition.key)
}

fn definition_for_target(target: BrowserTarget) -> Option<&'static BrowserDefinition> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.target == target)
}

fn definition_display_name(target: BrowserTarget) -> &'static str {
    match target {
        BrowserTarget::All => "a supported Chromium browser",
        BrowserTarget::Chrome => "Chrome",
        BrowserTarget::Edge => "Edge",
        BrowserTarget::Brave => "Brave",
        BrowserTarget::Vivaldi => "Vivaldi",
        BrowserTarget::Opera => "Opera",
        BrowserTarget::Chromium => "Chromium",
    }
}

fn find_browser_exe(definition: &BrowserDefinition) -> Option<PathBuf> {
    registry_app_path(definition.exe_name)
        .filter(|path| path.exists())
        .or_else(|| {
            candidate_paths(definition)
                .into_iter()
                .find(|path| path.exists())
        })
}

fn registry_app_path(exe_name: &str) -> Option<PathBuf> {
    let subkey = format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe_name}");
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .find_map(|hive| {
            RegKey::predef(hive)
                .open_subkey(&subkey)
                .ok()
                .and_then(|key| key.get_value::<String, _>("").ok())
                .map(PathBuf::from)
        })
}

fn candidate_paths(definition: &BrowserDefinition) -> Vec<PathBuf> {
    let bases = [
        std::env::var_os("ProgramFiles").map(PathBuf::from),
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
    ];

    bases
        .into_iter()
        .flatten()
        .flat_map(|base| {
            definition
                .relative_paths
                .iter()
                .map(move |relative| base.join(relative))
        })
        .collect()
}

fn query_value(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let mut output = String::new();
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte as char);
                    index += 3;
                } else {
                    output.push('%');
                    index += 1;
                }
            }
            byte => {
                output.push(byte as char);
                index += 1;
            }
        }
    }

    output
}

fn encode_query_value(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b',' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_query_contains_connection_state() {
        let query = setup_query(true);
        assert!(query.contains("connected=1"));
    }

    #[test]
    fn parses_browser_from_protocol_url() {
        let browser =
            browser_from_protocol_url("voice-watch://open-extensions?browser=brave").unwrap();
        assert_eq!(browser, BrowserTarget::Brave);
    }

    #[test]
    fn rejects_unknown_browser_from_protocol_url() {
        let error =
            browser_from_protocol_url("voice-watch://open-extensions?browser=firefox").unwrap_err();
        assert!(error.to_string().contains("unsupported browser target"));
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode_query_value("brave,edge"), "brave,edge");
        assert_eq!(encode_query_value("hello world"), "hello%20world");
    }

    #[test]
    fn chromium_forks_use_chromium_extensions_launch_url() {
        assert_eq!(
            definition_for_target(BrowserTarget::Brave)
                .unwrap()
                .launch_url,
            "chrome://extensions/"
        );
        assert_eq!(
            definition_for_target(BrowserTarget::Vivaldi)
                .unwrap()
                .launch_url,
            "chrome://extensions/"
        );
        assert_eq!(
            definition_for_target(BrowserTarget::Opera)
                .unwrap()
                .launch_url,
            "chrome://extensions/"
        );
    }
}
