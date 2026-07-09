use crate::messages::NATIVE_HOST_NAME;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserTarget {
    All,
    Chrome,
    Edge,
    Brave,
    Vivaldi,
    Opera,
    Chromium,
}

impl BrowserTarget {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "all" | "both" | "chromium-based" | "chromium_based" => Ok(Self::All),
            "chrome" | "google-chrome" => Ok(Self::Chrome),
            "edge" | "microsoft-edge" => Ok(Self::Edge),
            "brave" => Ok(Self::Brave),
            "vivaldi" => Ok(Self::Vivaldi),
            "opera" => Ok(Self::Opera),
            "chromium" => Ok(Self::Chromium),
            other => Err(anyhow!("unsupported browser target: {other}")),
        }
    }

    fn registry_paths(self) -> Vec<&'static str> {
        let all = vec![
            chrome_registry_path(),
            edge_registry_path(),
            brave_registry_path(),
            vivaldi_registry_path(),
            opera_chromium_registry_path(),
            opera_registry_path(),
            opera_gx_registry_path(),
            chromium_registry_path(),
        ];

        match self {
            Self::All => all,
            Self::Chrome => vec![chrome_registry_path()],
            Self::Edge => vec![edge_registry_path()],
            Self::Brave => vec![brave_registry_path()],
            Self::Vivaldi => vec![vivaldi_registry_path()],
            Self::Opera => vec![
                opera_chromium_registry_path(),
                opera_registry_path(),
                opera_gx_registry_path(),
            ],
            Self::Chromium => vec![chromium_registry_path()],
        }
    }
}

impl fmt::Display for BrowserTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::All => "all supported Chromium browsers",
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Brave => "Brave",
            Self::Vivaldi => "Vivaldi",
            Self::Opera => "Opera",
            Self::Chromium => "Chromium",
        };
        formatter.write_str(label)
    }
}

#[derive(Debug, Clone)]
pub struct RegistrationSummary {
    pub browser: BrowserTarget,
}

#[derive(Debug, Serialize)]
struct NativeHostManifest {
    name: &'static str,
    description: &'static str,
    path: String,
    #[serde(rename = "type")]
    kind: &'static str,
    allowed_origins: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExistingNativeHostManifest {
    #[serde(default)]
    allowed_origins: Vec<String>,
}

pub fn register_from_protocol_url(url: &str) -> Result<RegistrationSummary> {
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .context("setup link is missing query parameters")?;
    let extension_id = query_value(query, "extensionId")
        .or_else(|| query_value(query, "extension_id"))
        .context("setup link is missing the extension ID")?;
    let browser = query_value(query, "browser")
        .as_deref()
        .map(BrowserTarget::parse)
        .transpose()?
        .unwrap_or(BrowserTarget::All);

    register_native_host(&extension_id, browser, None)
}

pub fn register_native_host(
    extension_id: &str,
    browser: BrowserTarget,
    exe_path: Option<PathBuf>,
) -> Result<RegistrationSummary> {
    validate_extension_id(extension_id)?;

    let exe_path = match exe_path {
        Some(path) => path,
        None => std::env::current_exe().context("failed to locate Voice Watch executable")?,
    };
    let exe_path = exe_path
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", exe_path.display()))?;

    let manifest_path = manifest_path()?;
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let manifest = NativeHostManifest {
        name: NATIVE_HOST_NAME,
        description: "Voice Watch native messaging host",
        path: manifest_exe_path(&exe_path),
        kind: "stdio",
        allowed_origins: merged_allowed_origins(&manifest_path, extension_id),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, manifest_json)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let manifest_path = manifest_path.to_string_lossy().to_string();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for path in dedup_registry_paths(browser.registry_paths()) {
        let (key, _) = hkcu
            .create_subkey(path)
            .with_context(|| format!("failed to create HKCU\\{path}"))?;
        key.set_value("", &manifest_path)
            .with_context(|| format!("failed to write HKCU\\{path}"))?;
    }

    Ok(RegistrationSummary { browser })
}

fn merged_allowed_origins(manifest_path: &std::path::Path, extension_id: &str) -> Vec<String> {
    let new_origin = format!("chrome-extension://{extension_id}/");
    let mut origins = fs::read_to_string(manifest_path)
        .ok()
        .and_then(|text| serde_json::from_str::<ExistingNativeHostManifest>(&text).ok())
        .map(|manifest| manifest.allowed_origins)
        .unwrap_or_default();

    origins.push(new_origin);
    origins.sort();
    origins.dedup();
    origins
}

fn dedup_registry_paths(paths: Vec<&'static str>) -> Vec<&'static str> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.contains(&path) {
            deduped.push(path);
        }
    }
    deduped
}

fn validate_extension_id(extension_id: &str) -> Result<()> {
    let valid = extension_id.len() == 32
        && extension_id
            .chars()
            .all(|char_| ('a'..='p').contains(&char_));
    if valid {
        Ok(())
    } else {
        Err(anyhow!(
            "extension ID must be exactly 32 lowercase letters from a to p"
        ))
    }
}

fn query_value(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let mut output = String::new();
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte as char);
                    index += 3;
                } else {
                    output.push('%');
                    index += 1;
                }
            }
            byte => {
                output.push(byte as char);
                index += 1;
            }
        }
    }

    output
}

fn manifest_path() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .context("LOCALAPPDATA must be set on Windows")?;
    Ok(base
        .join("VoiceWatch")
        .join("native-messaging")
        .join(format!("{NATIVE_HOST_NAME}.json")))
}

fn manifest_exe_path(path: &std::path::Path) -> String {
    strip_windows_verbatim_prefix(&path.to_string_lossy())
}

fn strip_windows_verbatim_prefix(path: &str) -> String {
    path.strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| path.strip_prefix(r"\\?\").map(str::to_string))
        .unwrap_or_else(|| path.to_string())
}

fn chrome_registry_path() -> &'static str {
    r"Software\Google\Chrome\NativeMessagingHosts\com.voice_watch.native"
}

fn edge_registry_path() -> &'static str {
    r"Software\Microsoft\Edge\NativeMessagingHosts\com.voice_watch.native"
}

fn brave_registry_path() -> &'static str {
    r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.voice_watch.native"
}

fn vivaldi_registry_path() -> &'static str {
    r"Software\Vivaldi\NativeMessagingHosts\com.voice_watch.native"
}

fn opera_registry_path() -> &'static str {
    r"Software\Opera Software\Opera Stable\NativeMessagingHosts\com.voice_watch.native"
}

fn opera_gx_registry_path() -> &'static str {
    r"Software\Opera Software\Opera GX Stable\NativeMessagingHosts\com.voice_watch.native"
}

fn opera_chromium_registry_path() -> &'static str {
    chrome_registry_path()
}

fn chromium_registry_path() -> &'static str {
    r"Software\Chromium\NativeMessagingHosts\com.voice_watch.native"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_browser_targets() {
        assert_eq!(BrowserTarget::parse("all").unwrap(), BrowserTarget::All);
        assert_eq!(BrowserTarget::parse("both").unwrap(), BrowserTarget::All);
        assert_eq!(
            BrowserTarget::parse("Chrome").unwrap(),
            BrowserTarget::Chrome
        );
        assert_eq!(
            BrowserTarget::parse("microsoft-edge").unwrap(),
            BrowserTarget::Edge
        );
        assert_eq!(BrowserTarget::parse("brave").unwrap(), BrowserTarget::Brave);
        assert_eq!(
            BrowserTarget::parse("vivaldi").unwrap(),
            BrowserTarget::Vivaldi
        );
        assert_eq!(BrowserTarget::parse("opera").unwrap(), BrowserTarget::Opera);
        assert_eq!(
            BrowserTarget::parse("chromium").unwrap(),
            BrowserTarget::Chromium
        );
    }

    #[test]
    fn opera_uses_chrome_compatible_registry_path() {
        let paths = BrowserTarget::Opera.registry_paths();

        assert!(paths.contains(&chrome_registry_path()));
        assert!(paths.contains(&opera_registry_path()));
        assert!(paths.contains(&opera_gx_registry_path()));
    }

    #[test]
    fn all_browser_registration_paths_are_deduplicated() {
        let paths = dedup_registry_paths(BrowserTarget::All.registry_paths());

        assert_eq!(paths.len(), BrowserTarget::All.registry_paths().len() - 1);
        assert_eq!(
            paths
                .iter()
                .filter(|path| **path == chrome_registry_path())
                .count(),
            1
        );
    }

    #[test]
    fn allowed_origins_are_merged_from_existing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("com.voice_watch.native.json");
        fs::write(
            &manifest_path,
            r#"{
  "name": "com.voice_watch.native",
  "description": "Voice Watch native messaging host",
  "path": "C:\\VoiceWatch\\voice-watch.exe",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"
  ]
}"#,
        )
        .unwrap();

        let origins = merged_allowed_origins(&manifest_path, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");

        assert_eq!(
            origins,
            vec![
                "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/".to_string(),
                "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/".to_string(),
            ]
        );
    }

    #[test]
    fn protocol_registration_rejects_invalid_extension_id() {
        let error = register_from_protocol_url(
            "voice-watch://register-native-host?extensionId=not-valid&browser=chrome",
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("extension ID must be exactly 32 lowercase letters"));
    }

    #[test]
    fn strips_windows_verbatim_prefix_for_browser_manifest() {
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\C:\Users\Tommy\App\voice-watch.exe"),
            r"C:\Users\Tommy\App\voice-watch.exe"
        );
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\UNC\server\share\voice-watch.exe"),
            r"\\server\share\voice-watch.exe"
        );
    }
}
