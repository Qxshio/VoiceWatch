use crate::countdown::now_wall_clock_ms;
use crate::ipc;
use crate::messages::{
    AppMessage, ExtensionMessage, VoiceStatusData, VoiceStatusEnvelope, NATIVE_HOST_NAME,
    PROTOCOL_VERSION,
};
use crate::process;
use crate::settings;
use crate::settings::Settings;
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::io::{self, Read, Write};

const MAX_MESSAGE_BYTES: u32 = 1024 * 1024;

pub fn run_native_host() -> Result<()> {
    let settings = settings::load_settings().unwrap_or_default();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut microphone_status_published = false;

    while let Some(frame) = read_frame(&mut reader)? {
        let message = serde_json::from_slice::<ExtensionMessage>(&frame)
            .or_else(|_| decode_voice_status_fallback(&frame))
            .context("failed to decode extension message")?;

        match message {
            ExtensionMessage::Hello {
                protocol_version, ..
            } if protocol_version == PROTOCOL_VERSION => {
                let _ = ipc::publish_extension_connected();
                write_json(
                    &mut writer,
                    &AppMessage::HelloAck {
                        app_version: env!("CARGO_PKG_VERSION").to_string(),
                        protocol_version: PROTOCOL_VERSION,
                        poll_interval_seconds: settings.poll_interval_seconds,
                    },
                )?;
            }
            ExtensionMessage::Hello {
                protocol_version, ..
            } => {
                write_json(
                    &mut writer,
                    &AppMessage::Error {
                        message: format!(
                            "{NATIVE_HOST_NAME} expected protocol {PROTOCOL_VERSION}, got {protocol_version}"
                        ),
                    },
                )?;
            }
            ExtensionMessage::PollReadinessRequest { request_id } => {
                let readiness = current_poll_readiness(&settings, request_id);
                if readiness.microphone_active && !microphone_status_published {
                    let _ = ipc::publish_voice_status(microphone_restored_envelope(
                        readiness.request_id.clone(),
                    ));
                }
                microphone_status_published = readiness.microphone_active;

                write_json(
                    &mut writer,
                    &AppMessage::PollReadiness {
                        request_id: readiness.request_id,
                        should_poll: readiness.should_poll,
                        roblox_running: readiness.roblox_running,
                        roblox_playing: readiness.roblox_playing,
                        microphone_active: readiness.microphone_active,
                        reason: readiness.reason,
                        message: readiness.message,
                    },
                )?;
            }
            ExtensionMessage::Disconnect => {
                let _ = ipc::publish_extension_disconnected();
                write_json(
                    &mut writer,
                    &AppMessage::StatusAck {
                        request_id: None,
                        accepted: true,
                    },
                )?;
            }
            ExtensionMessage::VoiceStatus(envelope) => {
                let request_id = envelope.request_id.clone();
                let accepted = ipc::publish_voice_status(envelope).is_ok();
                write_json(
                    &mut writer,
                    &AppMessage::StatusAck {
                        request_id: Some(request_id),
                        accepted,
                    },
                )?;
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct PollReadiness {
    request_id: String,
    should_poll: bool,
    roblox_running: bool,
    roblox_playing: bool,
    microphone_active: bool,
    reason: Option<String>,
    message: Option<String>,
}

fn current_poll_readiness(settings: &Settings, request_id: String) -> PollReadiness {
    let presence = process::roblox_presence();

    if settings.pause_polling_while_roblox_uses_microphone && presence.microphone_active {
        return PollReadiness {
            request_id,
            should_poll: false,
            roblox_running: presence.process_running,
            roblox_playing: presence.game_window_visible,
            microphone_active: true,
            reason: Some("microphone_active".into()),
            message: Some("Roblox is using the microphone, so voice chat is active.".into()),
        };
    }

    if settings.only_poll_when_roblox_running && !presence.game_window_visible {
        return PollReadiness {
            request_id,
            should_poll: false,
            roblox_running: presence.process_running,
            roblox_playing: false,
            microphone_active: presence.microphone_active,
            reason: Some("roblox_not_playing".into()),
            message: Some("Waiting for a visible Roblox game window.".into()),
        };
    }

    PollReadiness {
        request_id,
        should_poll: true,
        roblox_running: presence.process_running,
        roblox_playing: presence.game_window_visible,
        microphone_active: presence.microphone_active,
        reason: None,
        message: None,
    }
}

fn microphone_restored_envelope(request_id: String) -> VoiceStatusEnvelope {
    VoiceStatusEnvelope {
        request_id,
        checked_at: now_wall_clock_ms(),
        ok: true,
        data: Some(VoiceStatusData {
            is_voice_enabled: true,
            is_user_opt_in: true,
            is_user_eligible: true,
            is_banned: false,
            ban_reason: None,
            banned_until_ms: None,
            denial_reason: None,
        }),
        error: None,
    }
}

pub fn read_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>> {
    let mut length_bytes = [0_u8; 4];
    match reader.read_exact(&mut length_bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error).context("failed to read native messaging frame length"),
    }

    let length = u32::from_le_bytes(length_bytes);
    if length > MAX_MESSAGE_BYTES {
        return Err(anyhow!(
            "native messaging frame is too large: {length} bytes"
        ));
    }

    let mut payload = vec![0_u8; length as usize];
    reader
        .read_exact(&mut payload)
        .context("failed to read native messaging frame payload")?;
    Ok(Some(payload))
}

pub fn write_json(writer: &mut impl Write, message: &impl Serialize) -> Result<()> {
    let payload = serde_json::to_vec(message).context("failed to serialize native message")?;
    if payload.len() > MAX_MESSAGE_BYTES as usize {
        return Err(anyhow!("native messaging response is too large"));
    }

    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .context("failed to write native messaging frame length")?;
    writer
        .write_all(&payload)
        .context("failed to write native messaging frame payload")?;
    writer.flush().context("failed to flush native message")?;
    Ok(())
}

fn decode_voice_status_fallback(frame: &[u8]) -> Result<ExtensionMessage> {
    let value = serde_json::from_slice::<serde_json::Value>(frame)?;
    if value.get("type").and_then(|kind| kind.as_str()) == Some("voice_status") {
        let envelope = serde_json::from_value::<VoiceStatusEnvelope>(value)?;
        return Ok(ExtensionMessage::VoiceStatus(envelope));
    }

    Err(anyhow!("unsupported native message"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_message_round_trips() {
        let mut bytes = Vec::new();
        write_json(
            &mut bytes,
            &AppMessage::CheckVoiceStatus {
                request_id: "abc".into(),
            },
        )
        .unwrap();

        let mut cursor = std::io::Cursor::new(bytes);
        let frame = read_frame(&mut cursor).unwrap().unwrap();
        assert!(String::from_utf8(frame)
            .unwrap()
            .contains("check_voice_status"));
    }

    #[test]
    fn disconnect_message_decodes() {
        let message = serde_json::from_str::<ExtensionMessage>(r#"{"type":"disconnect"}"#).unwrap();
        assert!(matches!(message, ExtensionMessage::Disconnect));
    }

    #[test]
    fn poll_readiness_message_decodes() {
        let message = serde_json::from_str::<ExtensionMessage>(
            r#"{"type":"poll_readiness_request","requestId":"probe"}"#,
        )
        .unwrap();
        assert!(matches!(
            message,
            ExtensionMessage::PollReadinessRequest { .. }
        ));
    }
}
