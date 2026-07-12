use crate::countdown::now_wall_clock_ms;
use crate::messages::VoiceStatusEnvelope;
use crate::process;
use crate::rejoin::LastServer;
use crate::settings;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const MAX_EVENT_LOG_BYTES: u64 = 1_000_000;
const DESKTOP_COMMAND_MAX_AGE_MS: i64 = 30_000;
const CONNECTION_STATE_VERSION: u32 = 2;
const MAX_EXTENSION_VERSION_LENGTH: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcEvent {
    VoiceStatus {
        envelope: VoiceStatusEnvelope,
        received_at_ms: i64,
    },
    ExtensionConnected {
        connected_at_ms: i64,
        #[serde(default)]
        extension_version: Option<String>,
    },
    ExtensionDisconnected {
        disconnected_at_ms: i64,
        #[serde(default)]
        still_connected: bool,
    },
    LastServer {
        server: LastServer,
        detected_at_ms: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionConnectionState {
    #[serde(default)]
    pub version: u32,
    pub last_connected_at_ms: i64,
    #[serde(default)]
    pub last_disconnected_at_ms: Option<i64>,
    #[serde(default)]
    pub active_host_pids: Vec<u32>,
    #[serde(default)]
    pub active_hosts: Vec<ExtensionHostState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionHostState {
    pub pid: u32,
    pub connected_at_ms: i64,
    #[serde(default)]
    pub extension_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeHostMarker {
    connected_at_ms: i64,
    #[serde(default)]
    extension_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopCommandEnvelope {
    requested_at_ms: i64,
    command: DesktopCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopCommand {
    CheckVoiceStatus {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    Rejoin {
        server: LastServer,
    },
    UpdateExtension {
        #[serde(rename = "desktopVersion")]
        desktop_version: String,
    },
}

impl ExtensionConnectionState {
    pub fn is_connected(&self) -> bool {
        if self.version >= CONNECTION_STATE_VERSION {
            return self
                .active_hosts
                .iter()
                .any(|host| process::is_current_executable_process(host.pid));
        }
        if self.version >= 1 {
            return self
                .active_host_pids
                .iter()
                .any(|pid| process::is_current_executable_process(*pid));
        }
        self.last_connected_at_ms > self.last_disconnected_at_ms.unwrap_or_default()
    }
}

pub fn publish_extension_connected(extension_version: &str) -> Result<()> {
    let connected_at_ms = now_wall_clock_ms();
    let current_pid = std::process::id();
    let extension_version = sanitize_extension_version(extension_version);
    write_host_marker(current_pid, connected_at_ms, extension_version.clone())?;
    let active_hosts = live_host_states();
    let active_host_pids = active_hosts.iter().map(|host| host.pid).collect();
    write_connection_state(&ExtensionConnectionState {
        version: CONNECTION_STATE_VERSION,
        last_connected_at_ms: connected_at_ms,
        last_disconnected_at_ms: None,
        active_host_pids,
        active_hosts,
    })?;
    publish_event(&IpcEvent::ExtensionConnected {
        connected_at_ms,
        extension_version,
    })
}

pub fn publish_extension_disconnected() -> Result<()> {
    let disconnected_at_ms = now_wall_clock_ms();
    let previous = read_connection_state().ok();
    let last_connected_at_ms = previous
        .as_ref()
        .map(|state| state.last_connected_at_ms)
        .unwrap_or_default();
    let current_pid = std::process::id();
    remove_host_marker(current_pid);
    let active_hosts = live_host_states();
    let active_host_pids = active_hosts.iter().map(|host| host.pid).collect::<Vec<_>>();
    let still_connected = !active_hosts.is_empty();

    write_connection_state(&ExtensionConnectionState {
        version: CONNECTION_STATE_VERSION,
        last_connected_at_ms,
        last_disconnected_at_ms: Some(disconnected_at_ms),
        active_host_pids,
        active_hosts,
    })?;
    publish_event(&IpcEvent::ExtensionDisconnected {
        disconnected_at_ms,
        still_connected,
    })
}

pub fn publish_voice_status(envelope: VoiceStatusEnvelope) -> Result<()> {
    write_voice_status(&envelope)?;
    publish_event(&IpcEvent::VoiceStatus {
        envelope,
        received_at_ms: now_wall_clock_ms(),
    })
}

pub fn read_voice_status() -> Option<VoiceStatusEnvelope> {
    let path = voice_status_path().ok()?;
    read_voice_status_from(&path)
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
    let mut state = read_connection_state().ok();
    let active_hosts = live_host_states();
    if active_hosts.is_empty() {
        if let Some(current) = state.as_mut().filter(|state| state.version >= 1) {
            current.active_host_pids.clear();
            current.active_hosts.clear();
        }
        return state;
    }

    let last_connected_at_ms = active_hosts
        .iter()
        .map(|host| host.connected_at_ms)
        .max()
        .unwrap_or_else(now_wall_clock_ms);
    let current = state.get_or_insert_with(|| ExtensionConnectionState {
        version: CONNECTION_STATE_VERSION,
        last_connected_at_ms,
        last_disconnected_at_ms: None,
        active_host_pids: Vec::new(),
        active_hosts: Vec::new(),
    });
    current.version = CONNECTION_STATE_VERSION;
    current.last_connected_at_ms = current.last_connected_at_ms.max(last_connected_at_ms);
    current.active_host_pids = active_hosts.iter().map(|host| host.pid).collect();
    current.active_hosts = active_hosts;
    state
}

pub fn request_voice_check() -> Result<String> {
    let requested_at_ms = now_wall_clock_ms();
    let request_id = format!("manual-{requested_at_ms}-{}", std::process::id());
    write_desktop_command(
        &desktop_command_path()?,
        DesktopCommand::CheckVoiceStatus {
            request_id: request_id.clone(),
        },
        requested_at_ms,
    )?;
    Ok(request_id)
}

pub fn request_rejoin(server: &LastServer) -> Result<()> {
    write_desktop_command(
        &desktop_command_path()?,
        DesktopCommand::Rejoin {
            server: server.clone(),
        },
        now_wall_clock_ms(),
    )
}

pub fn request_extension_update(host_pid: u32, desktop_version: &str) -> Result<()> {
    write_desktop_command(
        &targeted_desktop_command_path(host_pid)?,
        DesktopCommand::UpdateExtension {
            desktop_version: desktop_version.to_string(),
        },
        now_wall_clock_ms(),
    )
}

pub fn take_desktop_command() -> Result<Option<DesktopCommand>> {
    let pid = std::process::id();
    let now_ms = now_wall_clock_ms();
    let targeted_path = targeted_desktop_command_path(pid)?;
    let targeted_claim = targeted_path.with_file_name(format!("desktop-command-host-{pid}.claim"));
    if let Some(command) = take_desktop_command_from(&targeted_path, &targeted_claim, now_ms)? {
        return Ok(Some(command));
    }

    let shared_path = desktop_command_path()?;
    let shared_claim = shared_path.with_file_name(format!("desktop-command-{pid}.claim"));
    take_desktop_command_from(&shared_path, &shared_claim, now_ms)
}

pub fn event_log_len() -> u64 {
    event_log_path()
        .and_then(|path| Ok(fs::metadata(path)?.len()))
        .unwrap_or(0)
}

pub fn read_events_since(offset: u64) -> Result<(u64, Vec<IpcEvent>)> {
    let path = event_log_path()?;
    read_events_since_path(&path, offset)
}

fn read_events_since_path(path: &Path, offset: u64) -> Result<(u64, Vec<IpcEvent>)> {
    if !path.exists() {
        return Ok((0, Vec::new()));
    }

    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let file_len = file.metadata()?.len();
    let safe_offset = if offset <= file_len { offset } else { 0 };
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

fn live_host_states() -> Vec<ExtensionHostState> {
    let Ok(directory) = host_marker_dir() else {
        return Vec::new();
    };
    live_host_states_in(&directory)
}

fn live_host_states_in(directory: &Path) -> Vec<ExtensionHostState> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut hosts = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()
                .and_then(|name| name.to_str())
                .and_then(|name| name.parse::<u32>().ok())
                .map(|pid| (pid, entry.path()))
        })
        .filter_map(|(pid, path)| {
            if process::is_current_executable_process(pid) {
                Some(read_host_marker(&path, pid))
            } else {
                let _ = fs::remove_file(path);
                let _ = fs::remove_file(directory.join(format!("desktop-command-host-{pid}.json")));
                None
            }
        })
        .collect::<Vec<_>>();
    hosts.sort_by_key(|host| host.pid);
    hosts
}

fn read_host_marker(path: &Path, pid: u32) -> ExtensionHostState {
    let contents = fs::read_to_string(path).unwrap_or_default();
    if let Ok(marker) = serde_json::from_str::<NativeHostMarker>(&contents) {
        return ExtensionHostState {
            pid,
            connected_at_ms: marker.connected_at_ms,
            extension_version: marker
                .extension_version
                .as_deref()
                .and_then(sanitize_extension_version),
        };
    }

    ExtensionHostState {
        pid,
        connected_at_ms: contents.trim().parse().unwrap_or_default(),
        extension_version: None,
    }
}

fn write_host_marker(
    pid: u32,
    connected_at_ms: i64,
    extension_version: Option<String>,
) -> Result<()> {
    let path = host_marker_path(pid)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_vec(&NativeHostMarker {
        connected_at_ms,
        extension_version,
    })?;
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn remove_host_marker(pid: u32) {
    if let Ok(path) = host_marker_path(pid) {
        let _ = fs::remove_file(path);
    }
    if let Ok(path) = targeted_desktop_command_path(pid) {
        let _ = fs::remove_file(path);
    }
}

fn sanitize_extension_version(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= MAX_EXTENSION_VERSION_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.'))
    .then(|| value.to_string())
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

fn write_voice_status(envelope: &VoiceStatusEnvelope) -> Result<()> {
    let path = voice_status_path()?;
    write_voice_status_to(&path, envelope)
}

fn write_voice_status_to(path: &Path, envelope: &VoiceStatusEnvelope) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = serde_json::to_string_pretty(envelope)?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn read_voice_status_from(path: &Path) -> Option<VoiceStatusEnvelope> {
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_desktop_command(path: &Path, command: DesktopCommand, requested_at_ms: i64) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let request = DesktopCommandEnvelope {
        requested_at_ms,
        command,
    };
    let contents = serde_json::to_vec(&request)?;
    let temp_path = path.with_file_name(format!(
        "desktop-command-{}-{requested_at_ms}.tmp",
        std::process::id()
    ));
    fs::write(&temp_path, contents)
        .with_context(|| format!("failed to write {}", temp_path.display()))?;

    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(error).with_context(|| format!("failed to replace {}", path.display()));
        }
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error).with_context(|| format!("failed to publish {}", path.display()));
    }

    Ok(())
}

fn take_desktop_command_from(
    path: &Path,
    claim_path: &Path,
    now_ms: i64,
) -> Result<Option<DesktopCommand>> {
    let _ = fs::remove_file(claim_path);
    match fs::rename(path, claim_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_error) if !path.exists() => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to claim {}", path.display()))
        }
    }

    let result = (|| {
        let contents = fs::read_to_string(claim_path)
            .with_context(|| format!("failed to read {}", claim_path.display()))?;
        let request = serde_json::from_str::<DesktopCommandEnvelope>(&contents)
            .with_context(|| format!("failed to parse {}", claim_path.display()))?;
        let age_ms = now_ms.saturating_sub(request.requested_at_ms);
        if !(0..=DESKTOP_COMMAND_MAX_AGE_MS).contains(&age_ms) {
            return Ok(None);
        }

        if matches!(
            &request.command,
            DesktopCommand::CheckVoiceStatus { request_id } if request_id.trim().is_empty()
        ) {
            return Ok(None);
        }
        if matches!(
            &request.command,
            DesktopCommand::UpdateExtension { desktop_version }
                if sanitize_extension_version(desktop_version).is_none()
        ) {
            return Ok(None);
        }

        Ok(Some(request.command))
    })();
    let _ = fs::remove_file(claim_path);
    result
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

fn voice_status_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("voice-status.json"))
}

fn desktop_command_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("desktop-command.json"))
}

fn targeted_desktop_command_path(pid: u32) -> Result<PathBuf> {
    Ok(app_data_dir()?.join(format!("desktop-command-host-{pid}.json")))
}

fn host_marker_dir() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("native-hosts"))
}

fn host_marker_path(pid: u32) -> Result<PathBuf> {
    Ok(host_marker_dir()?.join(format!("{pid}.host")))
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
    use super::{
        live_host_states_in, read_voice_status_from, write_voice_status_to, NativeHostMarker,
    };
    use super::{
        read_events_since_path, take_desktop_command_from, write_desktop_command, DesktopCommand,
        ExtensionConnectionState, ExtensionHostState, IpcEvent, CONNECTION_STATE_VERSION,
        DESKTOP_COMMAND_MAX_AGE_MS,
    };
    use crate::messages::{VoiceStatusData, VoiceStatusEnvelope};
    use crate::rejoin::LastServer;
    use std::fs;

    #[test]
    fn connection_state_detects_active_connection() {
        let state = ExtensionConnectionState {
            version: 0,
            last_connected_at_ms: 20,
            last_disconnected_at_ms: None,
            active_host_pids: Vec::new(),
            active_hosts: Vec::new(),
        };

        assert!(state.is_connected());
    }

    #[test]
    fn connection_state_detects_newer_disconnect() {
        let state = ExtensionConnectionState {
            version: 0,
            last_connected_at_ms: 20,
            last_disconnected_at_ms: Some(30),
            active_host_pids: Vec::new(),
            active_hosts: Vec::new(),
        };

        assert!(!state.is_connected());
    }

    #[test]
    fn current_connection_state_requires_a_live_native_host() {
        let connected = ExtensionConnectionState {
            version: 1,
            last_connected_at_ms: 20,
            last_disconnected_at_ms: None,
            active_host_pids: vec![std::process::id()],
            active_hosts: Vec::new(),
        };
        let stale = ExtensionConnectionState {
            active_host_pids: vec![u32::MAX],
            ..connected.clone()
        };

        assert!(connected.is_connected());
        assert!(!stale.is_connected());
    }

    #[test]
    fn current_connection_state_uses_versioned_host_records() {
        let connected = ExtensionConnectionState {
            version: CONNECTION_STATE_VERSION,
            last_connected_at_ms: 20,
            last_disconnected_at_ms: None,
            active_host_pids: Vec::new(),
            active_hosts: vec![ExtensionHostState {
                pid: std::process::id(),
                connected_at_ms: 20,
                extension_version: Some("0.1.10".into()),
            }],
        };

        assert!(connected.is_connected());
    }

    #[test]
    fn native_host_markers_ignore_and_remove_stale_processes() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory
            .path()
            .join(format!("{}.host", std::process::id()));
        let stale = directory.path().join(format!("{}.host", u32::MAX));
        fs::write(&current, "current").unwrap();
        fs::write(&stale, "stale").unwrap();

        assert_eq!(
            live_host_states_in(directory.path())
                .into_iter()
                .map(|host| host.pid)
                .collect::<Vec<_>>(),
            vec![std::process::id()]
        );
        assert!(current.exists());
        assert!(!stale.exists());
    }

    #[test]
    fn native_host_markers_preserve_extension_versions() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory
            .path()
            .join(format!("{}.host", std::process::id()));
        let marker = NativeHostMarker {
            connected_at_ms: 1_000,
            extension_version: Some("0.1.9".into()),
        };
        fs::write(&current, serde_json::to_vec(&marker).unwrap()).unwrap();

        assert_eq!(
            live_host_states_in(directory.path()),
            vec![ExtensionHostState {
                pid: std::process::id(),
                connected_at_ms: 1_000,
                extension_version: Some("0.1.9".into()),
            }]
        );
    }

    #[test]
    fn desktop_command_is_claimed_once() {
        let directory = tempfile::tempdir().unwrap();
        let request_path = directory.path().join("desktop-command.json");
        let claim_path = directory.path().join("desktop-command.claim");
        let command = DesktopCommand::CheckVoiceStatus {
            request_id: "manual-test".into(),
        };
        write_desktop_command(&request_path, command.clone(), 1_000).unwrap();

        assert_eq!(
            take_desktop_command_from(&request_path, &claim_path, 1_001).unwrap(),
            Some(command)
        );
        assert_eq!(
            take_desktop_command_from(&request_path, &claim_path, 1_002).unwrap(),
            None
        );
    }

    #[test]
    fn stale_manual_voice_check_request_is_discarded() {
        let directory = tempfile::tempdir().unwrap();
        let request_path = directory.path().join("desktop-command.json");
        let claim_path = directory.path().join("desktop-command.claim");
        write_desktop_command(
            &request_path,
            DesktopCommand::CheckVoiceStatus {
                request_id: "manual-stale".into(),
            },
            1_000,
        )
        .unwrap();

        assert_eq!(
            take_desktop_command_from(
                &request_path,
                &claim_path,
                1_000 + DESKTOP_COMMAND_MAX_AGE_MS + 1,
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn rejoin_command_round_trips_through_the_desktop_queue() {
        let directory = tempfile::tempdir().unwrap();
        let request_path = directory.path().join("desktop-command.json");
        let claim_path = directory.path().join("desktop-command.claim");
        let command = DesktopCommand::Rejoin {
            server: LastServer {
                place_id: Some(123),
                game_instance_id: Some("1bb8dd1d-ad4c-43d2-a9c6-63feee836e43".into()),
                access_code: None,
                link_code: None,
                detected_at_ms: 1_000,
            },
        };
        write_desktop_command(&request_path, command.clone(), 1_000).unwrap();

        assert_eq!(
            take_desktop_command_from(&request_path, &claim_path, 1_001).unwrap(),
            Some(command)
        );
    }

    #[test]
    fn extension_update_command_round_trips_through_a_targeted_queue() {
        let directory = tempfile::tempdir().unwrap();
        let request_path = directory.path().join("desktop-command-host-42.json");
        let claim_path = directory.path().join("desktop-command-host-42.claim");
        let command = DesktopCommand::UpdateExtension {
            desktop_version: "0.1.10".into(),
        };
        write_desktop_command(&request_path, command.clone(), 1_000).unwrap();

        assert_eq!(
            take_desktop_command_from(&request_path, &claim_path, 1_001).unwrap(),
            Some(command)
        );
    }

    #[test]
    fn event_reader_restarts_after_log_rotation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        let event = IpcEvent::ExtensionConnected {
            connected_at_ms: 1_000,
            extension_version: Some("0.1.10".into()),
        };
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&event).unwrap()),
        )
        .unwrap();

        let (offset, events) = read_events_since_path(&path, 1_000_000).unwrap();
        assert_eq!(offset, std::fs::metadata(path).unwrap().len());
        assert!(matches!(
            events.as_slice(),
            [IpcEvent::ExtensionConnected {
                connected_at_ms: 1_000,
                ..
            }]
        ));
    }

    #[test]
    fn voice_status_cache_round_trips_sanitized_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("voice-status.json");
        let status = VoiceStatusEnvelope {
            request_id: "cached".into(),
            checked_at: 1_000,
            ok: true,
            data: Some(VoiceStatusData {
                is_voice_enabled: false,
                is_user_opt_in: true,
                is_user_eligible: false,
                is_banned: true,
                ban_reason: Some(7),
                banned_until_ms: Some(10_000),
                denial_reason: Some(6),
            }),
            error: None,
        };
        write_voice_status_to(&path, &status).unwrap();

        let cached = read_voice_status_from(&path).unwrap();
        assert_eq!(cached.request_id, status.request_id);
        assert_eq!(cached.data, status.data);
    }
}
