use crate::rejoin::{open_rejoin_target, LastServer};
use anyhow::Result;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

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

    let result = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title("Voice chat restored")
        .set_description(description)
        .set_buttons(if can_rejoin {
            MessageButtons::OkCancel
        } else {
            MessageButtons::Ok
        })
        .show();

    let action = match result {
        MessageDialogResult::Ok if can_rejoin => OverlayAction::RejoinLastServer,
        _ => OverlayAction::Dismiss,
    };

    if let (OverlayAction::RejoinLastServer, Some(server)) = (action, last_server) {
        open_rejoin_target(server)?;
    }

    Ok(action)
}

