use anyhow::{Context, Result};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;

const TRAY_MUTEX_NAME: &str = "Local\\VoiceWatch.TrayApp";

pub struct SingleInstanceGuard {
    handle: HANDLE,
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

pub fn acquire_tray_instance() -> Result<Option<SingleInstanceGuard>> {
    acquire_named(TRAY_MUTEX_NAME)
}

fn acquire_named(name: &str) -> Result<Option<SingleInstanceGuard>> {
    let name = wide(name);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error())
            .context("failed to create single-instance guard");
    }

    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already_running {
        unsafe {
            CloseHandle(handle);
        }
        return Ok(None);
    }

    Ok(Some(SingleInstanceGuard { handle }))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_named_guard_returns_none() {
        let name = format!("Local\\VoiceWatch.Test.{}", std::process::id());
        let first = acquire_named(&name).unwrap();
        assert!(first.is_some());

        let second = acquire_named(&name).unwrap();
        assert!(second.is_none());
    }
}
