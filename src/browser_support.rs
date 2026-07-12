use std::path::PathBuf;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

struct BrowserDefinition {
    key: &'static str,
    app_path_name: Option<&'static str>,
    relative_paths: &'static [&'static str],
}

const DEFINITIONS: &[BrowserDefinition] = &[
    BrowserDefinition {
        key: "brave",
        app_path_name: Some("brave.exe"),
        relative_paths: &[
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Programs\BraveSoftware\Brave-Browser\Application\brave.exe",
        ],
    },
    BrowserDefinition {
        key: "chrome",
        app_path_name: Some("chrome.exe"),
        relative_paths: &[
            r"Google\Chrome\Application\chrome.exe",
            r"Programs\Google\Chrome\Application\chrome.exe",
        ],
    },
    BrowserDefinition {
        key: "edge",
        app_path_name: Some("msedge.exe"),
        relative_paths: &[
            r"Microsoft\Edge\Application\msedge.exe",
            r"Programs\Microsoft\Edge\Application\msedge.exe",
        ],
    },
    BrowserDefinition {
        key: "vivaldi",
        app_path_name: Some("vivaldi.exe"),
        relative_paths: &[
            r"Vivaldi\Application\vivaldi.exe",
            r"Programs\Vivaldi\Application\vivaldi.exe",
        ],
    },
    BrowserDefinition {
        key: "opera",
        app_path_name: Some("opera.exe"),
        relative_paths: &[
            r"Programs\Opera\opera.exe",
            r"Opera\opera.exe",
            r"Opera GX\opera.exe",
            r"Programs\Opera GX\opera.exe",
        ],
    },
    BrowserDefinition {
        key: "chromium",
        // Chrome and Chromium share chrome.exe, so App Paths cannot distinguish them.
        app_path_name: None,
        relative_paths: &[
            r"Chromium\Application\chrome.exe",
            r"Programs\Chromium\Application\chrome.exe",
        ],
    },
];

pub fn setup_query(is_connected: bool) -> String {
    let installed = installed_browsers();
    let browsers = installed
        .iter()
        .map(|definition| definition.key)
        .collect::<Vec<_>>()
        .join(",");
    let preferred = installed
        .first()
        .map(|definition| definition.key)
        .unwrap_or_default();

    format!(
        "browsers={}&preferred={}&connected={}",
        encode_query_value(&browsers),
        encode_query_value(preferred),
        if is_connected { "1" } else { "0" }
    )
}

pub fn extension_update_query(desktop_version: &str, extension_version: &str) -> String {
    format!(
        "{}&mode=update&desktopVersion={}&extensionVersion={}",
        setup_query(true),
        encode_query_value(desktop_version),
        encode_query_value(extension_version)
    )
}

fn installed_browsers() -> Vec<&'static BrowserDefinition> {
    DEFINITIONS
        .iter()
        .filter(|definition| find_browser_exe(definition).is_some())
        .collect()
}

fn find_browser_exe(definition: &BrowserDefinition) -> Option<PathBuf> {
    definition
        .app_path_name
        .and_then(registry_app_path)
        .filter(|path| path.exists())
        .or_else(|| candidate_paths(definition).find(|path| path.exists()))
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

fn candidate_paths(definition: &BrowserDefinition) -> impl Iterator<Item = PathBuf> + use<'_> {
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .flat_map(|base| {
            definition
                .relative_paths
                .iter()
                .map(move |relative| base.join(relative))
        })
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
    fn encodes_query_values() {
        assert_eq!(encode_query_value("brave,edge"), "brave,edge");
        assert_eq!(encode_query_value("hello world"), "hello%20world");
    }

    #[test]
    fn extension_update_query_contains_both_versions() {
        let query = extension_update_query("0.1.10", "0.1.9");
        assert!(query.contains("mode=update"));
        assert!(query.contains("desktopVersion=0.1.10"));
        assert!(query.contains("extensionVersion=0.1.9"));
    }

    #[test]
    fn chromium_detection_does_not_reuse_chromes_app_path() {
        let chromium = DEFINITIONS
            .iter()
            .find(|definition| definition.key == "chromium")
            .unwrap();

        assert_eq!(chromium.app_path_name, None);
    }
}
