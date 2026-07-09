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
use std::time::{Duration, Instant};

const MAX_MESSAGE_BYTES: u32 = 1024 * 1024;
const SMART_POLLING_MIC_QUIET_SECONDS: u64 = 20;

pub fn run_native_host() -> Result<()> {
    let mut session = NativeSession::new(settings::load_settings().unwrap_or_default());
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

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
                        poll_interval_seconds: session.settings.poll_interval_seconds,
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
                session.reload_settings();
                let readiness = session.current_poll_readiness(request_id);
                if readiness.microphone_active && !session.microphone_status_published {
                    let _ = ipc::publish_voice_status(microphone_restored_envelope(
                        readiness.request_id.clone(),
                    ));
                }
                session.microphone_status_published = readiness.microphone_active;

                write_json(
                    &mut writer,
                    &AppMessage::PollReadiness {
                        request_id: readiness.request_id,
                        poll_interval_seconds: readiness.poll_interval_seconds,
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
            ExtensionMessage::LastServer { server } => {
                let accepted = ipc::publish_last_server(server).is_ok();
                write_json(&mut writer, &AppMessage::LastServerAck { accepted })?;
            }
            ExtensionMessage::VoiceStatus(envelope) => {
                let request_id = envelope.request_id.clone();
                session.remember_voice_status(&envelope);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastVoiceStatus {
    Unknown,
    NotSuspended,
    Suspended,
}

#[derive(Debug)]
struct NativeSession {
    settings: Settings,
    microphone_status_published: bool,
    microphone_quiet_since: Option<Instant>,
    last_voice_status: LastVoiceStatus,
}

impl NativeSession {
    fn new(settings: Settings) -> Self {
        Self {
            settings,
            microphone_status_published: false,
            microphone_quiet_since: None,
            last_voice_status: LastVoiceStatus::Unknown,
        }
    }

    fn reload_settings(&mut self) {
        if let Ok(settings) = settings::load_settings() {
            self.settings = settings;
        }
    }

    fn remember_voice_status(&mut self, envelope: &VoiceStatusEnvelope) {
        let Some(data) = envelope.data.as_ref().filter(|_| envelope.ok) else {
            return;
        };

        self.last_voice_status = if data.is_banned {
            LastVoiceStatus::Suspended
        } else {
            LastVoiceStatus::NotSuspended
        };
    }

    fn current_poll_readiness(&mut self, request_id: String) -> PollReadiness {
        self.current_poll_readiness_for_presence(
            request_id,
            process::roblox_presence(),
            Instant::now(),
        )
    }

    fn current_poll_readiness_for_presence(
        &mut self,
        request_id: String,
        presence: process::RobloxPresence,
        now: Instant,
    ) -> PollReadiness {
        if presence.microphone_active {
            self.microphone_quiet_since = None;
            return PollReadiness {
                request_id,
                poll_interval_seconds: self.settings.poll_interval_seconds,
                should_poll: !self.settings.pause_polling_while_roblox_uses_microphone,
                roblox_running: presence.process_running,
                roblox_playing: presence.game_window_visible,
                microphone_active: true,
                reason: self
                    .settings
                    .pause_polling_while_roblox_uses_microphone
                    .then(|| "microphone_active".into()),
                message: self
                    .settings
                    .pause_polling_while_roblox_uses_microphone
                    .then(|| "Roblox is using the microphone, so voice chat is active.".into()),
            };
        }

        if !presence.game_window_visible {
            self.microphone_quiet_since = None;
        }

        if self.settings.only_poll_when_roblox_running && !presence.game_window_visible {
            return PollReadiness {
                request_id,
                poll_interval_seconds: self.settings.poll_interval_seconds,
                should_poll: false,
                roblox_running: presence.process_running,
                roblox_playing: false,
                microphone_active: false,
                reason: Some("roblox_not_playing".into()),
                message: Some("Waiting for a visible Roblox game window.".into()),
            };
        }

        let microphone_quiet_for = if presence.game_window_visible {
            let quiet_since = self.microphone_quiet_since.get_or_insert(now);
            now.checked_duration_since(*quiet_since).unwrap_or_default()
        } else {
            Duration::default()
        };

        if self.settings.smart_polling
            && self.last_voice_status == LastVoiceStatus::NotSuspended
            && microphone_quiet_for >= Duration::from_secs(SMART_POLLING_MIC_QUIET_SECONDS)
        {
            return PollReadiness {
                request_id,
                poll_interval_seconds: self.settings.poll_interval_seconds,
                should_poll: false,
                roblox_running: presence.process_running,
                roblox_playing: presence.game_window_visible,
                microphone_active: false,
                reason: Some("smart_polling_mic_quiet".into()),
                message: Some("Smart polling paused checks while Roblox is muted.".into()),
            };
        }

        PollReadiness {
            request_id,
            poll_interval_seconds: self.settings.poll_interval_seconds,
            should_poll: true,
            roblox_running: presence.process_running,
            roblox_playing: presence.game_window_visible,
            microphone_active: false,
            reason: None,
            message: None,
        }
    }
}

#[derive(Debug)]
struct PollReadiness {
    request_id: String,
    poll_interval_seconds: u64,
    should_poll: bool,
    roblox_running: bool,
    roblox_playing: bool,
    microphone_active: bool,
    reason: Option<String>,
    message: Option<String>,
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

    #[test]
    fn smart_polling_pauses_after_quiet_mic_when_not_suspended() {
        let mut session = NativeSession::new(Settings::default());
        session.remember_voice_status(&voice_status(false));

        let now = Instant::now();
        let first =
            session.current_poll_readiness_for_presence("first".into(), presence(false, true), now);
        let second = session.current_poll_readiness_for_presence(
            "second".into(),
            presence(false, true),
            now + Duration::from_secs(SMART_POLLING_MIC_QUIET_SECONDS + 1),
        );

        assert!(first.should_poll);
        assert!(!second.should_poll);
        assert_eq!(second.reason.as_deref(), Some("smart_polling_mic_quiet"));
    }

    #[test]
    fn smart_polling_keeps_polling_while_suspended() {
        let mut session = NativeSession::new(Settings::default());
        session.remember_voice_status(&voice_status(true));

        let now = Instant::now();
        let _ =
            session.current_poll_readiness_for_presence("first".into(), presence(false, true), now);
        let second = session.current_poll_readiness_for_presence(
            "second".into(),
            presence(false, true),
            now + Duration::from_secs(SMART_POLLING_MIC_QUIET_SECONDS + 1),
        );

        assert!(second.should_poll);
    }

    #[test]
    fn smart_polling_quiet_timer_resets_when_microphone_is_active() {
        let settings = Settings {
            pause_polling_while_roblox_uses_microphone: false,
            ..Settings::default()
        };
        let mut session = NativeSession::new(settings);
        session.remember_voice_status(&voice_status(false));

        let now = Instant::now();
        let _ =
            session.current_poll_readiness_for_presence("first".into(), presence(false, true), now);
        let active = session.current_poll_readiness_for_presence(
            "active".into(),
            presence(true, true),
            now + Duration::from_secs(5),
        );
        let muted_again = session.current_poll_readiness_for_presence(
            "muted-again".into(),
            presence(false, true),
            now + Duration::from_secs(SMART_POLLING_MIC_QUIET_SECONDS + 1),
        );

        assert!(active.should_poll);
        assert!(muted_again.should_poll);
    }

    fn presence(microphone_active: bool, game_window_visible: bool) -> process::RobloxPresence {
        process::RobloxPresence {
            process_running: true,
            game_window_visible,
            microphone_active,
        }
    }

    fn voice_status(is_banned: bool) -> VoiceStatusEnvelope {
        VoiceStatusEnvelope {
            request_id: "test".into(),
            checked_at: now_wall_clock_ms(),
            ok: true,
            data: Some(VoiceStatusData {
                is_voice_enabled: !is_banned,
                is_user_opt_in: true,
                is_user_eligible: true,
                is_banned,
                ban_reason: if is_banned { Some(7) } else { None },
                banned_until_ms: if is_banned {
                    Some(now_wall_clock_ms() + 120_000)
                } else {
                    None
                },
                denial_reason: if is_banned { Some(6) } else { None },
            }),
            error: None,
        }
    }
}
