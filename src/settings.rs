use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const APP_DIR_NAME: &str = "Voice Watch";
const SETTINGS_FILE_NAME: &str = "settings.json";
const MIN_POLL_SECONDS: u64 = 10;
const MAX_POLL_SECONDS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub poll_interval_seconds: u64,
    pub only_poll_when_roblox_running: bool,
    #[serde(default = "default_true")]
    pub pause_polling_while_roblox_uses_microphone: bool,
    pub show_overlay: bool,
    pub play_sound_on_restore: bool,
    pub overlay_position: OverlayPosition,
    #[serde(default = "default_true")]
    pub launch_on_startup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    TopRight,
    BottomRight,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval_seconds: 10,
            only_poll_when_roblox_running: true,
            pause_polling_while_roblox_uses_microphone: true,
            show_overlay: true,
            play_sound_on_restore: true,
            overlay_position: OverlayPosition::TopRight,
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
    if !path.exists() {
        let settings = Settings::default();
        save_settings(&settings)?;
        return Ok(settings);
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read settings at {}", path.display()))?;
    let settings = serde_json::from_str::<Settings>(&contents)
        .with_context(|| format!("failed to parse settings at {}", path.display()))?
        .validate();
    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(&settings.clone().validate())?;
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn settings_path() -> Result<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .context("APPDATA or LOCALAPPDATA must be set on Windows")?;

    Ok(base.join(APP_DIR_NAME).join(SETTINGS_FILE_NAME))
}

fn default_true() -> bool {
    true
}
