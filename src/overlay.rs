use crate::rejoin::{open_rejoin_target, LastServer};
use anyhow::Result;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDOK, MB_ICONINFORMATION, MB_OK, MB_OKCANCEL,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAction {
    Dismiss,
    RejoinLastServer,
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

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
