use crate::messages::{
    AppMessage, ExtensionMessage, VoiceStatusEnvelope, NATIVE_HOST_NAME, PROTOCOL_VERSION,
};
use crate::settings;
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

    while let Some(frame) = read_frame(&mut reader)? {
        let message = serde_json::from_slice::<ExtensionMessage>(&frame)
            .or_else(|_| decode_voice_status_fallback(&frame))
            .context("failed to decode extension message")?;

        match message {
            ExtensionMessage::Hello {
                protocol_version, ..
            } if protocol_version == PROTOCOL_VERSION => {
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
            ExtensionMessage::VoiceStatus(envelope) => {
                // The tray app IPC bridge will consume this in the next milestone. For now the
                // host acknowledges sanitized status so extension development can proceed safely.
                write_json(
                    &mut writer,
                    &AppMessage::StatusAck {
                        request_id: Some(envelope.request_id),
                        accepted: true,
                    },
                )?;
            }
        }
    }

    Ok(())
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
}
