use std::collections::HashSet;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM, RECT};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible,
};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

pub const ROBLOX_PLAYER_PROCESS: &str = "RobloxPlayerBeta.exe";

const MICROPHONE_CONSENT_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged";
const LAST_USED_TIME_START: &str = "LastUsedTimeStart";
const LAST_USED_TIME_STOP: &str = "LastUsedTimeStop";
const MAX_PROCESS_PATH_UNITS: usize = 32_768;

static CACHED_ROBLOX_WINDOW: AtomicIsize = AtomicIsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RobloxPresence {
    pub process_running: bool,
    pub game_window_visible: bool,
    pub microphone_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowBounds {
    pub fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }
}

pub fn roblox_presence() -> RobloxPresence {
    let processes = roblox_player_processes();
    let process_ids = processes
        .iter()
        .map(|process| process.pid)
        .collect::<HashSet<_>>();

    RobloxPresence {
        process_running: !processes.is_empty(),
        game_window_visible: find_visible_window_for_processes(&process_ids).is_some(),
        microphone_active: is_roblox_microphone_active(
            processes
                .iter()
                .filter_map(|process| process.microphone_registry_key.as_deref()),
        ),
    }
}

pub fn roblox_window_bounds() -> Option<WindowBounds> {
    if let Some(hwnd) = cached_visible_window() {
        return window_bounds(hwnd);
    }

    let process_ids = roblox_player_processes()
        .into_iter()
        .map(|process| process.pid)
        .collect::<HashSet<_>>();
    find_visible_window_for_processes(&process_ids).and_then(window_bounds)
}

pub fn is_current_executable_process(pid: u32) -> bool {
    let Some(process_path) = process_executable_path(pid) else {
        return false;
    };
    let Ok(current_path) = std::env::current_exe() else {
        return false;
    };

    normalized_windows_path(&process_path) == normalized_windows_path(&current_path)
}

struct RobloxProcess {
    pid: u32,
    microphone_registry_key: Option<String>,
}

fn roblox_player_processes() -> Vec<RobloxProcess> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut processes = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;

    while has_entry {
        if wide_buffer_eq_ignore_ascii_case(&entry.szExeFile, ROBLOX_PLAYER_PROCESS) {
            let microphone_registry_key = process_executable_path(entry.th32ProcessID)
                .as_deref()
                .and_then(microphone_registry_key_from_path);
            processes.push(RobloxProcess {
                pid: entry.th32ProcessID,
                microphone_registry_key,
            });
        }
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }

    unsafe {
        CloseHandle(snapshot);
    }
    processes
}

fn process_executable_path(pid: u32) -> Option<PathBuf> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }

    let mut buffer = vec![0_u16; MAX_PROCESS_PATH_UNITS];
    let mut length = buffer.len() as u32;
    let succeeded =
        unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) != 0 };
    unsafe {
        CloseHandle(process);
    }

    succeeded.then(|| PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
}

fn wide_buffer_eq_ignore_ascii_case(buffer: &[u16], expected: &str) -> bool {
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length]).eq_ignore_ascii_case(expected)
}

fn normalized_windows_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn find_visible_window_for_processes(process_ids: &HashSet<u32>) -> Option<HWND> {
    if process_ids.is_empty() {
        CACHED_ROBLOX_WINDOW.store(0, Ordering::Relaxed);
        return None;
    }

    if let Some(hwnd) = cached_visible_window() {
        let mut process_id = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut process_id);
        }
        if process_ids.contains(&process_id) {
            return Some(hwnd);
        }
    }

    let mut search = WindowSearch {
        process_ids,
        hwnd: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(
            Some(enum_visible_process_window),
            (&mut search as *mut WindowSearch).cast::<std::ffi::c_void>() as LPARAM,
        );
    }

    CACHED_ROBLOX_WINDOW.store(search.hwnd as isize, Ordering::Relaxed);
    (!search.hwnd.is_null()).then_some(search.hwnd)
}

fn cached_visible_window() -> Option<HWND> {
    let hwnd = CACHED_ROBLOX_WINDOW.load(Ordering::Relaxed) as HWND;
    if hwnd.is_null() {
        return None;
    }

    let visible = unsafe {
        IsWindow(hwnd) != 0 && IsWindowVisible(hwnd) != 0 && GetWindowTextLengthW(hwnd) > 0
    };
    if visible {
        Some(hwnd)
    } else {
        CACHED_ROBLOX_WINDOW.store(0, Ordering::Relaxed);
        None
    }
}

struct WindowSearch<'a> {
    process_ids: &'a HashSet<u32>,
    hwnd: HWND,
}

unsafe extern "system" fn enum_visible_process_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam as *mut WindowSearch);

    if IsWindowVisible(hwnd) == 0 || GetWindowTextLengthW(hwnd) <= 0 {
        return 1;
    }

    let mut process_id = 0;
    GetWindowThreadProcessId(hwnd, &mut process_id);
    if !search.process_ids.contains(&process_id) {
        return 1;
    }

    search.hwnd = hwnd;
    0
}

fn window_bounds(hwnd: HWND) -> Option<WindowBounds> {
    let mut rect = RECT::default();
    let ok = unsafe { GetWindowRect(hwnd, &mut rect) };
    (ok != 0).then_some(WindowBounds {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn is_roblox_microphone_active<'a>(keys: impl Iterator<Item = &'a str>) -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(non_packaged) = hkcu.open_subkey(MICROPHONE_CONSENT_SUBKEY) else {
        return false;
    };

    keys.into_iter().any(|key_name| {
        non_packaged.open_subkey(key_name).ok().is_some_and(|key| {
            let start = key.get_value::<u64, _>(LAST_USED_TIME_START).unwrap_or(0);
            let stop = key.get_value::<u64, _>(LAST_USED_TIME_STOP).unwrap_or(0);
            microphone_entry_is_active(start, stop)
        })
    })
}

fn microphone_registry_key_from_path(path: &Path) -> Option<String> {
    let path = path.to_string_lossy();
    (!path.is_empty()).then(|| path.replace('\\', "#"))
}

fn microphone_entry_is_active(start: u64, stop: u64) -> bool {
    start > 0 && stop <= start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_registry_key_matches_windows_privacy_shape() {
        assert_eq!(
            microphone_registry_key_from_path(Path::new(
                r"C:\Users\Tommy\AppData\Local\Roblox\Versions\version-1\RobloxPlayerBeta.exe"
            ))
            .unwrap(),
            r"C:#Users#Tommy#AppData#Local#Roblox#Versions#version-1#RobloxPlayerBeta.exe"
        );
    }

    #[test]
    fn microphone_usage_is_active_until_stop_exceeds_start() {
        assert!(microphone_entry_is_active(100, 0));
        assert!(microphone_entry_is_active(100, 99));
        assert!(microphone_entry_is_active(100, 100));
        assert!(!microphone_entry_is_active(100, 101));
        assert!(!microphone_entry_is_active(0, 0));
    }

    #[test]
    fn process_names_are_read_from_null_terminated_windows_buffers() {
        let mut buffer = [0_u16; 260];
        let name = ROBLOX_PLAYER_PROCESS.encode_utf16().collect::<Vec<_>>();
        buffer[..name.len()].copy_from_slice(&name);

        assert!(wide_buffer_eq_ignore_ascii_case(
            &buffer,
            "robloxplayerbeta.EXE"
        ));
    }

    #[test]
    fn current_process_matches_the_running_executable() {
        assert!(is_current_executable_process(std::process::id()));
    }

    #[test]
    fn roblox_presence_probe_is_safe_when_the_client_is_absent() {
        let presence = roblox_presence();
        if !presence.process_running {
            assert!(!presence.game_window_visible);
            assert!(!presence.microphone_active);
        }
    }
}
