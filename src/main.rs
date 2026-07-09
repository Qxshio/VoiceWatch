#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_state;
mod browser_support;
mod countdown;
mod ipc;
mod messages;
mod native_host_registration;
mod native_messaging;
mod overlay;
mod process;
mod rejoin;
mod roblox_logs;
mod settings;
mod settings_window;
mod tray;
mod updates;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if is_browser_native_host_invocation(&args) {
        return native_messaging::run_native_host();
    }

    let mut args = args.into_iter();

    match args.next().as_deref() {
        Some("--native-host") => native_messaging::run_native_host(),
        Some("--register-native-host") => {
            let extension_id = args
                .next()
                .context("expected extension ID after --register-native-host")?;
            let browser = read_browser_arg(args.collect::<Vec<_>>().as_slice())?;
            native_host_registration::register_native_host(&extension_id, browser, None)?;
            println!("Registered native host for {browser}");
            Ok(())
        }
        Some("--print-config-path") => {
            println!("{}", settings::settings_path()?.display());
            Ok(())
        }
        Some("--simulate-suspension") => {
            let seconds = args
                .next()
                .as_deref()
                .unwrap_or("30")
                .parse::<u64>()
                .context("expected seconds after --simulate-suspension")?;
            tray::run_simulated_countdown(seconds)
        }
        Some("--apply-update") => updates::run_update_helper_from_args(args.collect::<Vec<_>>()),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) if other.starts_with("voice-watch://") => handle_protocol_url(other),
        Some(other) => anyhow::bail!("unknown argument: {other}"),
        None => tray::run_tray_app(),
    }
}

fn is_browser_native_host_invocation(args: &[String]) -> bool {
    args.first()
        .is_some_and(|first| first.starts_with("chrome-extension://"))
}

fn handle_protocol_url(url: &str) -> Result<()> {
    if url.starts_with("voice-watch://open-extensions") {
        let browser = browser_support::browser_from_protocol_url(url)?;
        browser_support::open_extensions_page(browser)?;
        return Ok(());
    }

    match native_host_registration::register_from_protocol_url(url) {
        Ok(summary) => {
            show_message(
                "Voice Watch",
                &format!(
                    "Desktop connection registered for {}.\n\nReturn to your browser and open the Voice Watch extension popup. If it still shows the old status, close and reopen your browser once.",
                    summary.browser
                ),
            );
            Ok(())
        }
        Err(error) => {
            show_message("Voice Watch setup failed", &format!("{error:#}"));
            Err(error)
        }
    }
}

fn read_browser_arg(args: &[String]) -> Result<native_host_registration::BrowserTarget> {
    let Some(index) = args.iter().position(|arg| arg == "--browser") else {
        return Ok(native_host_registration::BrowserTarget::All);
    };

    let browser = args
        .get(index + 1)
        .context("expected browser after --browser")?;
    native_host_registration::BrowserTarget::parse(browser)
}

fn show_message(title: &str, body: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    let title = wide(title);
    let body = wide(body);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn print_help() {
    println!(
        "Voice Watch\n\
\n\
Usage:\n\
  voice-watch.exe                     Run the tray app\n\
  voice-watch.exe --native-host        Run as a Chromium native messaging host\n\
  voice-watch.exe --register-native-host <extension-id> [--browser all|chrome|edge|brave|vivaldi|opera|chromium]\n\
                                       Register browser native messaging\n\
  voice-watch.exe --simulate-suspension <seconds>\n\
                                       Demo countdown and restore notification\n\
  voice-watch.exe --print-config-path  Print the settings file location\n"
    );
}
