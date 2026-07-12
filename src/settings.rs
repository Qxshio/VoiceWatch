use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const APP_DIR_NAME: &str = "Voice Watch";
const SETTINGS_FILE_NAME: &str = "settings.json";
const MIN_POLL_SECONDS: u64 = 10;
const MAX_POLL_SECONDS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_poll_interval_seconds")]
    pub poll_interval_seconds: u64,
    #[serde(default = "default_true")]
    pub only_poll_when_roblox_running: bool,
    #[serde(default)]
    pub developer_mode: bool,
    #[serde(default = "default_true")]
    pub pause_polling_while_roblox_uses_microphone: bool,
    #[serde(default = "default_true")]
    pub smart_polling: bool,
    #[serde(default = "default_true")]
    pub show_overlay: bool,
    #[serde(default = "default_true")]
    pub play_sound_on_restore: bool,
    #[serde(default = "default_true")]
    pub launch_on_startup: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval_seconds: default_poll_interval_seconds(),
            only_poll_when_roblox_running: true,
            developer_mode: false,
            pause_polling_while_roblox_uses_microphone: true,
            smart_polling: true,
            show_overlay: true,
            play_sound_on_restore: true,
            launch_on_startup: true,
        }
    }
}

impl Settings {
    pub fn validate(mut self) -> Self {
        self.poll_interval_seconds = self
            .poll_interval_seconds
            .clamp(MIN_POLL_SECONDS, MAX_POLL_SECONDS);
        self
    }
}

pub fn load_settings() -> Result<Settings> {
    let path = settings_path()?;
    load_settings_from(&path)
}

fn load_settings_from(path: &Path) -> Result<Settings> {
    if !path.exists() {
        let settings = Settings::default();
        save_settings_to(path, &settings)?;
        return Ok(settings);
    }

    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read settings at {}", path.display()))?;
    let raw_settings = match serde_json::from_str::<serde_json::Value>(&contents) {
        Ok(settings) => settings,
        Err(error) => return recover_invalid_settings(path, &error.to_string()),
    };
    let needs_default_fields = settings_needs_default_fields(&raw_settings);
    let settings = match serde_json::from_value::<Settings>(raw_settings) {
        Ok(settings) => settings.validate(),
        Err(error) => return recover_invalid_settings(path, &error.to_string()),
    };
    if needs_default_fields {
        if let Err(error) = save_settings_to(path, &settings) {
            eprintln!("Failed to update settings defaults: {error:#}");
        }
    }
    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let path = settings_path()?;
    save_settings_to(&path, settings)
}

fn save_settings_to(path: &Path, settings: &Settings) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(&settings.clone().validate())?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn recover_invalid_settings(path: &Path, reason: &str) -> Result<Settings> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let backup = path.with_file_name(format!(
        "settings.invalid-{timestamp}-{}.json",
        std::process::id()
    ));
    fs::rename(path, &backup).with_context(|| {
        format!(
            "failed to preserve invalid settings at {} ({reason})",
            path.display()
        )
    })?;

    eprintln!(
        "Invalid Voice Watch settings were moved to {}: {reason}",
        backup.display()
    );
    let settings = Settings::default();
    save_settings_to(path, &settings)?;
    Ok(settings)
}

pub fn settings_path() -> Result<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .context("APPDATA or LOCALAPPDATA must be set on Windows")?;

    Ok(base.join(APP_DIR_NAME).join(SETTINGS_FILE_NAME))
}

pub fn modified_at() -> Option<SystemTime> {
    settings_path()
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .and_then(|metadata| metadata.modified().ok())
}

pub fn apply_launch_on_startup(enabled: bool) -> Result<()> {
    const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_VALUE_NAME: &str = "Voice Watch";

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) = hkcu
        .create_subkey(RUN_SUBKEY)
        .context("failed to open Windows startup registry key")?;

    if enabled {
        let exe_path =
            std::env::current_exe().context("failed to locate Voice Watch executable")?;
        run_key
            .set_value(RUN_VALUE_NAME, &format!("\"{}\"", exe_path.display()))
            .context("failed to enable Voice Watch startup")?;
    } else {
        match run_key.delete_value(RUN_VALUE_NAME) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("failed to disable Voice Watch startup"),
        }
    }

    Ok(())
}

fn default_poll_interval_seconds() -> u64 {
    10
}

fn default_true() -> bool {
    true
}

fn settings_needs_default_fields(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };

    object.contains_key("overlayPosition")
        || [
            "pollIntervalSeconds",
            "onlyPollWhenRobloxRunning",
            "developerMode",
            "pausePollingWhileRobloxUsesMicrophone",
            "smartPolling",
            "showOverlay",
            "playSoundOnRestore",
            "launchOnStartup",
        ]
        .iter()
        .any(|key| !object.contains_key(*key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_settings_missing_defaulted_fields() {
        let old_settings = json!({
            "pollIntervalSeconds": 10,
            "onlyPollWhenRobloxRunning": true,
            "showOverlay": true,
            "playSoundOnRestore": true,
        });

        assert!(settings_needs_default_fields(&old_settings));
    }

    #[test]
    fn complete_settings_do_not_need_defaulted_fields() {
        let settings = serde_json::to_value(Settings::default()).unwrap();

        assert!(!settings_needs_default_fields(&settings));
    }

    #[test]
    fn stale_overlay_position_triggers_rewrite() {
        let mut settings = serde_json::to_value(Settings::default()).unwrap();
        settings["overlayPosition"] = json!("top-right");

        assert!(settings_needs_default_fields(&settings));
    }

    #[test]
    fn invalid_settings_are_backed_up_and_reset() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "{not valid json").unwrap();

        let settings = load_settings_from(&path).unwrap();

        assert_eq!(settings, Settings::default());
        assert!(
            serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path).unwrap()).is_ok()
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.invalid-"))
                .count(),
            1
        );
    }
}
