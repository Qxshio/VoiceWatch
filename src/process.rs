use std::collections::HashSet;
use std::path::Path;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

pub const ROBLOX_PLAYER_PROCESS: &str = "RobloxPlayerBeta.exe";

const MICROPHONE_CONSENT_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged";
const LAST_USED_TIME_START: &str = "LastUsedTimeStart";
const LAST_USED_TIME_STOP: &str = "LastUsedTimeStop";

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
    let microphone_keys = processes
        .iter()
        .filter_map(|process| process.microphone_registry_key.as_deref())
        .collect::<HashSet<_>>();

    RobloxPresence {
        process_running: !processes.is_empty(),
        game_window_visible: has_visible_window_for_processes(&process_ids),
        microphone_active: is_roblox_microphone_active(&microphone_keys),
    }
}

pub fn is_roblox_running() -> bool {
    roblox_presence().game_window_visible
}

pub fn roblox_window_bounds() -> Option<WindowBounds> {
    let processes = roblox_player_processes();
    let process_ids = processes
        .iter()
        .map(|process| process.pid)
        .collect::<HashSet<_>>();
    let hwnd = find_visible_window_for_processes(&process_ids)?;
    window_bounds(hwnd)
}

struct RobloxProcess {
    pid: u32,
    microphone_registry_key: Option<String>,
}

fn roblox_player_processes() -> Vec<RobloxProcess> {
    let refresh = RefreshKind::new().with_processes(ProcessRefreshKind::new());
    let mut system = System::new_with_specifics(refresh);
    system.refresh_processes();

    system
        .processes()
        .values()
        .filter(|process| process.name().eq_ignore_ascii_case(ROBLOX_PLAYER_PROCESS))
        .map(|process| RobloxProcess {
            pid: process.pid().as_u32(),
            microphone_registry_key: process.exe().and_then(microphone_registry_key_from_path),
        })
        .collect()
}

fn has_visible_window_for_processes(process_ids: &HashSet<u32>) -> bool {
    find_visible_window_for_processes(process_ids).is_some()
}

fn find_visible_window_for_processes(process_ids: &HashSet<u32>) -> Option<HWND> {
    if process_ids.is_empty() {
        return None;
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

    (!search.hwnd.is_null()).then_some(search.hwnd)
}

struct WindowSearch<'a> {
    process_ids: &'a HashSet<u32>,
    hwnd: HWND,
}

unsafe extern "system" fn enum_visible_process_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam as *mut WindowSearch);

    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }

    let mut process_id = 0;
    GetWindowThreadProcessId(hwnd, &mut process_id);
    if !search.process_ids.contains(&process_id) {
        return 1;
    }

    if GetWindowTextLengthW(hwnd) <= 0 {
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

fn is_roblox_microphone_active(current_microphone_keys: &HashSet<&str>) -> bool {
    if current_microphone_keys.is_empty() {
        return false;
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(non_packaged) = hkcu.open_subkey(MICROPHONE_CONSENT_SUBKEY) else {
        return false;
    };

    non_packaged.enum_keys().flatten().any(|key_name| {
        current_microphone_keys.contains(key_name.as_str())
            && non_packaged.open_subkey(&key_name).ok().is_some_and(|key| {
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
}
