use crate::countdown::now_wall_clock_ms;
use crate::messages::VoiceStatusEnvelope;
use crate::rejoin::LastServer;
use crate::settings;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

const MAX_EVENT_LOG_BYTES: u64 = 1_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcEvent {
    VoiceStatus {
        envelope: VoiceStatusEnvelope,
        received_at_ms: i64,
    },
    ExtensionConnected {
        connected_at_ms: i64,
    },
    ExtensionDisconnected {
        disconnected_at_ms: i64,
    },
    LastServer {
        server: LastServer,
        detected_at_ms: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionConnectionState {
    pub last_connected_at_ms: i64,
    #[serde(default)]
    pub last_disconnected_at_ms: Option<i64>,
}

impl ExtensionConnectionState {
    pub fn is_connected(&self) -> bool {
        self.last_connected_at_ms > self.last_disconnected_at_ms.unwrap_or_default()
    }
}

pub fn publish_extension_connected() -> Result<()> {
    let connected_at_ms = now_wall_clock_ms();
    write_connection_state(&ExtensionConnectionState {
        last_connected_at_ms: connected_at_ms,
        last_disconnected_at_ms: None,
    })?;
    publish_event(&IpcEvent::ExtensionConnected { connected_at_ms })
}

pub fn publish_extension_disconnected() -> Result<()> {
    let disconnected_at_ms = now_wall_clock_ms();
    let last_connected_at_ms = read_connection_state()
        .map(|state| state.last_connected_at_ms)
        .unwrap_or_default();

    write_connection_state(&ExtensionConnectionState {
        last_connected_at_ms,
        last_disconnected_at_ms: Some(disconnected_at_ms),
    })?;
    publish_event(&IpcEvent::ExtensionDisconnected { disconnected_at_ms })
}

pub fn publish_voice_status(envelope: VoiceStatusEnvelope) -> Result<()> {
    publish_event(&IpcEvent::VoiceStatus {
        envelope,
        received_at_ms: now_wall_clock_ms(),
    })
}

pub fn publish_last_server(mut server: LastServer) -> Result<()> {
    if server.detected_at_ms <= 0 {
        server.detected_at_ms = now_wall_clock_ms();
    }
    write_last_server(&server)?;
    publish_event(&IpcEvent::LastServer {
        detected_at_ms: server.detected_at_ms,
        server,
    })
}

pub fn read_last_server() -> Option<LastServer> {
    let path = last_server_path().ok()?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn read_extension_connection_state() -> Option<ExtensionConnectionState> {
    read_connection_state().ok()
}

pub fn event_log_len() -> u64 {
    event_log_path()
        .and_then(|path| Ok(fs::metadata(path)?.len()))
        .unwrap_or(0)
}

pub fn read_events_since(offset: u64) -> Result<(u64, Vec<IpcEvent>)> {
    let path = event_log_path()?;
    if !path.exists() {
        return Ok((0, Vec::new()));
    }

    let mut file = OpenOptions::new()
        .read(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let file_len = file.metadata()?.len();
    let safe_offset = offset.min(file_len);
    file.seek(SeekFrom::Start(safe_offset))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let events = contents
        .lines()
        .filter_map(|line| serde_json::from_str::<IpcEvent>(line).ok())
        .collect::<Vec<_>>();

    Ok((file_len, events))
}

fn publish_event(event: &IpcEvent) -> Result<()> {
    let path = event_log_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    rotate_event_log_if_needed(&path)?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn rotate_event_log_if_needed(path: &std::path::Path) -> Result<()> {
    if fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        <= MAX_EVENT_LOG_BYTES
    {
        return Ok(());
    }

    fs::write(path, b"").with_context(|| format!("failed to rotate {}", path.display()))
}

fn read_connection_state() -> Result<ExtensionConnectionState> {
    let path = connection_state_path()?;
    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&contents).context("failed to parse extension connection state")
}

fn write_connection_state(state: &ExtensionConnectionState) -> Result<()> {
    let path = connection_state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = serde_json::to_string_pretty(state)?;
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn write_last_server(server: &LastServer) -> Result<()> {
    let path = last_server_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = serde_json::to_string_pretty(server)?;
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn event_log_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("ipc-events.jsonl"))
}

fn connection_state_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("extension-state.json"))
}

fn last_server_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("last-server.json"))
}

fn app_data_dir() -> Result<PathBuf> {
    let settings_path = settings::settings_path()?;
    settings_path
        .parent()
        .map(PathBuf::from)
        .context("settings path has no parent directory")
}

#[cfg(test)]
mod tests {
    use super::ExtensionConnectionState;

    #[test]
    fn connection_state_detects_active_connection() {
        let state = ExtensionConnectionState {
            last_connected_at_ms: 20,
            last_disconnected_at_ms: None,
        };

        assert!(state.is_connected());
    }

    #[test]
    fn connection_state_detects_newer_disconnect() {
        let state = ExtensionConnectionState {
            last_connected_at_ms: 20,
            last_disconnected_at_ms: Some(30),
        };

        assert!(!state.is_connected());
    }
}
