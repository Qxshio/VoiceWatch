#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_state;
mod countdown;
mod ipc;
mod messages;
mod monitor;
mod native_messaging;
mod overlay;
mod process;
mod rejoin;
mod roblox_logs;
mod settings;
mod tray;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if is_browser_native_host_invocation(&args) {
        return native_messaging::run_native_host();
    }

    let mut args = args.into_iter();

    match args.next().as_deref() {
        Some("--native-host") => native_messaging::run_native_host(),
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
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown argument: {other}"),
        None => tray::run_tray_app(),
    }
}

fn is_browser_native_host_invocation(args: &[String]) -> bool {
    args.first()
        .is_some_and(|first| first.starts_with("chrome-extension://"))
}

fn print_help() {
    println!(
        "Voice Watch\n\
\n\
Usage:\n\
  voice-watch.exe                     Run the tray app\n\
  voice-watch.exe --native-host        Run as a Chrome/Edge native messaging host\n\
  voice-watch.exe --simulate-suspension <seconds>\n\
                                       Demo countdown and restore notification\n\
  voice-watch.exe --print-config-path  Print the settings file location\n"
    );
}
