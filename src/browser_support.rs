use crate::native_host_registration::BrowserTarget;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

#[derive(Debug, Clone)]
pub struct BrowserInstall {
    pub target: BrowserTarget,
}

struct BrowserDefinition {
    target: BrowserTarget,
    key: &'static str,
    exe_name: &'static str,
    extensions_url: &'static str,
    relative_paths: &'static [&'static str],
}

const DEFINITIONS: &[BrowserDefinition] = &[
    BrowserDefinition {
        target: BrowserTarget::Brave,
        key: "brave",
        exe_name: "brave.exe",
        extensions_url: "brave://extensions",
        relative_paths: &[
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Programs\BraveSoftware\Brave-Browser\Application\brave.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Chrome,
        key: "chrome",
        exe_name: "chrome.exe",
        extensions_url: "chrome://extensions",
        relative_paths: &[
            r"Google\Chrome\Application\chrome.exe",
            r"Programs\Google\Chrome\Application\chrome.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Edge,
        key: "edge",
        exe_name: "msedge.exe",
        extensions_url: "edge://extensions",
        relative_paths: &[
            r"Microsoft\Edge\Application\msedge.exe",
            r"Programs\Microsoft\Edge\Application\msedge.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Vivaldi,
        key: "vivaldi",
        exe_name: "vivaldi.exe",
        extensions_url: "vivaldi://extensions",
        relative_paths: &[
            r"Vivaldi\Application\vivaldi.exe",
            r"Programs\Vivaldi\Application\vivaldi.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Opera,
        key: "opera",
        exe_name: "opera.exe",
        extensions_url: "opera://extensions",
        relative_paths: &[
            r"Programs\Opera\opera.exe",
            r"Opera\opera.exe",
            r"Opera GX\opera.exe",
            r"Programs\Opera GX\opera.exe",
        ],
    },
    BrowserDefinition {
        target: BrowserTarget::Chromium,
        key: "chromium",
        exe_name: "chrome.exe",
        extensions_url: "chrome://extensions",
        relative_paths: &[
            r"Chromium\Application\chrome.exe",
            r"Programs\Chromium\Application\chrome.exe",
        ],
    },
];

pub fn installed_browsers() -> Vec<BrowserInstall> {
    DEFINITIONS
        .iter()
        .filter_map(|definition| {
            find_browser_exe(definition).map(|_| BrowserInstall {
                target: definition.target,
            })
        })
        .collect()
}

pub fn setup_query(is_connected: bool) -> String {
    let installed = installed_browsers();
    let browsers = installed
        .iter()
        .filter_map(|install| browser_key(install.target))
        .collect::<Vec<_>>()
        .join(",");
    let preferred = installed
        .first()
        .and_then(|install| browser_key(install.target))
        .unwrap_or_default();
    format!(
        "browsers={}&preferred={}&connected={}",
        encode_query_value(&browsers),
        encode_query_value(preferred),
        if is_connected { "1" } else { "0" }
    )
}

pub fn open_extensions_page(browser: BrowserTarget) -> Result<()> {
    let target = match browser {
        BrowserTarget::All => installed_browsers()
            .first()
            .map(|install| install.target)
            .context("no supported Chromium browser was found")?,
        specific => specific,
    };
    let definition = definition_for_target(target)
        .with_context(|| format!("unsupported browser target: {target}"))?;
    let exe_path = find_browser_exe(definition)
        .with_context(|| format!("{} is not installed", definition_display_name(target)))?;

    Command::new(&exe_path)
        .arg(definition.extensions_url)
        .spawn()
        .with_context(|| format!("failed to open {}", definition_display_name(target)))?;
    Ok(())
}

pub fn browser_from_protocol_url(url: &str) -> Result<BrowserTarget> {
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .context("setup link is missing query parameters")?;
    let browser = query_value(query, "browser").unwrap_or_else(|| "all".into());
    BrowserTarget::parse(&browser)
}

pub fn browser_key(target: BrowserTarget) -> Option<&'static str> {
    definition_for_target(target).map(|definition| definition.key)
}

fn definition_for_target(target: BrowserTarget) -> Option<&'static BrowserDefinition> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.target == target)
}

fn definition_display_name(target: BrowserTarget) -> &'static str {
    match target {
        BrowserTarget::All => "a supported Chromium browser",
        BrowserTarget::Chrome => "Chrome",
        BrowserTarget::Edge => "Edge",
        BrowserTarget::Brave => "Brave",
        BrowserTarget::Vivaldi => "Vivaldi",
        BrowserTarget::Opera => "Opera",
        BrowserTarget::Chromium => "Chromium",
    }
}

fn find_browser_exe(definition: &BrowserDefinition) -> Option<PathBuf> {
    registry_app_path(definition.exe_name)
        .filter(|path| path.exists())
        .or_else(|| {
            candidate_paths(definition)
                .into_iter()
                .find(|path| path.exists())
        })
}

fn registry_app_path(exe_name: &str) -> Option<PathBuf> {
    let subkey = format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe_name}");
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .find_map(|hive| {
            RegKey::predef(hive)
                .open_subkey(&subkey)
                .ok()
                .and_then(|key| key.get_value::<String, _>("").ok())
                .map(PathBuf::from)
        })
}

fn candidate_paths(definition: &BrowserDefinition) -> Vec<PathBuf> {
    let bases = [
        std::env::var_os("ProgramFiles").map(PathBuf::from),
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
    ];

    bases
        .into_iter()
        .flatten()
        .flat_map(|base| {
            definition
                .relative_paths
                .iter()
                .map(move |relative| base.join(relative))
        })
        .collect()
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

fn encode_query_value(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b',' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_query_contains_connection_state() {
        let query = setup_query(true);
        assert!(query.contains("connected=1"));
    }

    #[test]
    fn parses_browser_from_protocol_url() {
        let browser =
            browser_from_protocol_url("voice-watch://open-extensions?browser=brave").unwrap();
        assert_eq!(browser, BrowserTarget::Brave);
    }

    #[test]
    fn rejects_unknown_browser_from_protocol_url() {
        let error =
            browser_from_protocol_url("voice-watch://open-extensions?browser=firefox").unwrap_err();
        assert!(error.to_string().contains("unsupported browser target"));
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode_query_value("brave,edge"), "brave,edge");
        assert_eq!(encode_query_value("hello world"), "hello%20world");
    }
}
