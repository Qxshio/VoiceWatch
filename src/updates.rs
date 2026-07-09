use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

const RELEASES_URL: &str = "https://api.github.com/repos/Qxshio/VoiceWatch/releases?per_page=10";
const USER_AGENT: &str = "VoiceWatch";
const GITHUB_API_VERSION: &str = "2022-11-28";
const UPDATE_HELPER_EXE: &str = "voice-watch-updater.exe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub tag_name: String,
    pub release_url: String,
    pub installer_name: String,
    pub installer_url: String,
}

#[derive(Debug, Clone)]
pub enum UpdateEvent {
    Available(UpdateInfo),
    InstallLaunched,
    InstallFailed { info: UpdateInfo, message: String },
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ReleaseVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

pub fn check_for_update() -> Result<Option<UpdateInfo>> {
    let response = ureq::get(RELEASES_URL)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", USER_AGENT)
        .set("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .timeout(Duration::from_secs(15))
        .call()
        .context("failed to contact GitHub releases")?;

    let body = response
        .into_string()
        .context("failed to read GitHub release response")?;
    let releases = serde_json::from_str::<Vec<GitHubRelease>>(&body)
        .context("failed to parse GitHub release response")?;

    Ok(select_update(
        releases,
        env!("CARGO_PKG_VERSION"),
        installer_asset_name,
    ))
}

pub fn download_and_launch_update(info: &UpdateInfo) -> Result<()> {
    let installer_path = download_installer(info)?;
    launch_update_helper(&installer_path)
}

pub fn run_update_helper_from_args(args: Vec<String>) -> Result<()> {
    match run_update_helper_from_args_inner(args) {
        Ok(()) => Ok(()),
        Err(error) => {
            show_update_error(&format!("{error:#}"));
            Err(error)
        }
    }
}

fn run_update_helper_from_args_inner(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let installer_path = args
        .next()
        .map(PathBuf::from)
        .context("expected installer path after --apply-update")?;
    let wait_pid = read_option_u32(&mut args, "--wait-pid")?;
    let restart_exe = read_option_path(&mut args, "--restart-exe")?;

    if let Some(pid) = wait_pid {
        wait_for_process_exit(pid, Duration::from_secs(60));
    }

    run_installer(&installer_path)?;

    if let Some(restart_exe) = restart_exe {
        Command::new(&restart_exe)
            .spawn()
            .with_context(|| format!("failed to reopen {}", restart_exe.display()))?;
    }

    Ok(())
}

fn select_update<F>(
    releases: Vec<GitHubRelease>,
    current_version: &str,
    asset_name_for_version: F,
) -> Option<UpdateInfo>
where
    F: Fn(&str) -> String,
{
    let current = ReleaseVersion::parse(current_version)?;
    releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version = ReleaseVersion::parse_tag(&release.tag_name)?;
            if version <= current {
                return None;
            }

            let version_label = version.to_string();
            let installer_name = asset_name_for_version(&version_label);
            let asset = release
                .assets
                .iter()
                .find(|asset| asset.name == installer_name)?;

            Some((
                version,
                UpdateInfo {
                    version: version_label,
                    tag_name: release.tag_name,
                    release_url: release.html_url,
                    installer_name: asset.name.clone(),
                    installer_url: asset.browser_download_url.clone(),
                },
            ))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, info)| info)
}

fn download_installer(info: &UpdateInfo) -> Result<PathBuf> {
    let update_dir = std::env::temp_dir()
        .join("VoiceWatch")
        .join("updates")
        .join(&info.version);
    fs::create_dir_all(&update_dir)
        .with_context(|| format!("failed to create {}", update_dir.display()))?;

    let installer_path = update_dir.join(&info.installer_name);
    let response = ureq::get(&info.installer_url)
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(120))
        .call()
        .with_context(|| format!("failed to download {}", info.installer_name))?;

    let mut reader = response.into_reader();
    let mut file = fs::File::create(&installer_path)
        .with_context(|| format!("failed to create {}", installer_path.display()))?;
    let bytes = io::copy(&mut reader, &mut file)
        .with_context(|| format!("failed to write {}", installer_path.display()))?;
    if bytes == 0 {
        bail!("downloaded installer was empty");
    }

    Ok(installer_path)
}

fn launch_update_helper(installer_path: &Path) -> Result<()> {
    let current_exe = std::env::current_exe().context("failed to locate Voice Watch executable")?;
    let helper_dir = std::env::temp_dir().join("VoiceWatch").join("updater");
    fs::create_dir_all(&helper_dir)
        .with_context(|| format!("failed to create {}", helper_dir.display()))?;

    let helper_path = helper_dir.join(UPDATE_HELPER_EXE);
    fs::copy(&current_exe, &helper_path).with_context(|| {
        format!(
            "failed to copy update helper from {} to {}",
            current_exe.display(),
            helper_path.display()
        )
    })?;

    Command::new(&helper_path)
        .arg("--apply-update")
        .arg(installer_path)
        .arg("--wait-pid")
        .arg(std::process::id().to_string())
        .arg("--restart-exe")
        .arg(&current_exe)
        .spawn()
        .with_context(|| format!("failed to start {}", helper_path.display()))?;

    Ok(())
}

fn run_installer(installer_path: &Path) -> Result<()> {
    if !installer_path.exists() {
        bail!("installer does not exist: {}", installer_path.display());
    }

    let status = Command::new(installer_path)
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/SP-",
            "/CLOSEAPPLICATIONS",
        ])
        .status()
        .with_context(|| format!("failed to run {}", installer_path.display()))?;

    if !status.success() {
        bail!("installer exited with status {status}");
    }

    Ok(())
}

fn read_option_u32<I>(args: &mut I, name: &str) -> Result<Option<u32>>
where
    I: Iterator<Item = String>,
{
    let Some(flag) = args.next() else {
        return Ok(None);
    };
    if flag != name {
        return Err(anyhow!("unexpected update helper argument: {flag}"));
    }
    let value = args
        .next()
        .with_context(|| format!("expected value after {name}"))?;
    value
        .parse::<u32>()
        .with_context(|| format!("invalid value after {name}: {value}"))
        .map(Some)
}

fn read_option_path<I>(args: &mut I, name: &str) -> Result<Option<PathBuf>>
where
    I: Iterator<Item = String>,
{
    let Some(flag) = args.next() else {
        return Ok(None);
    };
    if flag != name {
        return Err(anyhow!("unexpected update helper argument: {flag}"));
    }
    args.next()
        .map(PathBuf::from)
        .with_context(|| format!("expected value after {name}"))
        .map(Some)
}

fn wait_for_process_exit(pid: u32, timeout: Duration) {
    let milliseconds = timeout.as_millis().min(u32::MAX as u128) as u32;
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return;
        }
        WaitForSingleObject(handle, milliseconds);
        CloseHandle(handle);
    }
}

fn installer_asset_name(version: &str) -> String {
    format!("VoiceWatch-{version}-Setup.exe")
}

impl ReleaseVersion {
    fn parse_tag(tag: &str) -> Option<Self> {
        Self::parse(tag.trim_start_matches('v'))
    }

    fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        parts.next().is_none().then_some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn show_update_error(body: &str) {
    let title = wide("Voice Watch update failed");
    let body = wide(body);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_versions_order_numerically() {
        assert!(ReleaseVersion::parse_tag("v0.10.0") > ReleaseVersion::parse_tag("v0.2.9"));
    }

    #[test]
    fn selects_highest_newer_release_with_matching_installer() {
        let selected = select_update(
            vec![
                release("v0.1.3", false, vec!["VoiceWatch-0.1.3-Setup.exe"]),
                release("v0.2.0", false, vec!["notes.txt"]),
                release("v0.1.4", true, vec!["VoiceWatch-0.1.4-Setup.exe"]),
            ],
            "0.1.2",
            installer_asset_name,
        )
        .unwrap();

        assert_eq!(selected.version, "0.1.3");
        assert_eq!(selected.installer_name, "VoiceWatch-0.1.3-Setup.exe");
    }

    #[test]
    fn ignores_current_and_older_releases() {
        let selected = select_update(
            vec![release("v0.1.2", false, vec!["VoiceWatch-0.1.2-Setup.exe"])],
            "0.1.2",
            installer_asset_name,
        );

        assert!(selected.is_none());
    }

    fn release(tag_name: &str, draft: bool, assets: Vec<&str>) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag_name.into(),
            html_url: format!("https://github.com/Qxshio/VoiceWatch/releases/tag/{tag_name}"),
            draft,
            assets: assets
                .into_iter()
                .map(|name| GitHubAsset {
                    name: name.into(),
                    browser_download_url: format!(
                        "https://github.com/Qxshio/VoiceWatch/releases/download/{tag_name}/{name}"
                    ),
                })
                .collect(),
        }
    }
}
