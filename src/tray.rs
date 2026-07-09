use crate::app_state::{AppState, VoiceState};
use crate::browser_support;
use crate::countdown::{format_remaining, now_wall_clock_ms};
use crate::ipc::{self, IpcEvent};
use crate::messages::VoiceStatusData;
use crate::overlay;
use crate::process;
use crate::roblox_logs;
use crate::settings;
use anyhow::{Context, Result};
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

#[derive(Debug, Clone)]
enum UserEvent {
    Menu(MenuEvent),
    Ipc(IpcEvent),
    Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MenuShape {
    browser_connected: bool,
    has_countdown: bool,
}

pub fn run_tray_app() -> Result<()> {
    let settings = settings::load_settings().context("failed to load settings")?;
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

    let ipc_proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        let mut offset = ipc::event_log_len();
        loop {
            std::thread::sleep(Duration::from_millis(750));
            let Ok((new_offset, events)) = ipc::read_events_since(offset) else {
                continue;
            };
            offset = new_offset;
            for event in events {
                let _ = ipc_proxy.send_event(UserEvent::Ipc(event));
            }
        }
    });

    let tick_proxy = event_loop.create_proxy();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let _ = tick_proxy.send_event(UserEvent::Tick);
    });

    let mut app = TrayApp::new(settings);
    event_loop.run_app(&mut app)?;
    Ok(())
}

pub fn run_simulated_countdown(seconds: u64) -> Result<()> {
    let banned_until_ms = now_wall_clock_ms() + (seconds as i64 * 1000);
    let mut state = AppState::default();
    state.apply_voice_status_data(
        now_wall_clock_ms(),
        VoiceStatusData {
            is_voice_enabled: false,
            is_user_opt_in: true,
            is_user_eligible: false,
            is_banned: true,
            ban_reason: Some(7),
            banned_until_ms: Some(banned_until_ms),
            denial_reason: Some(6),
        },
    );

    while let Some(countdown) = &state.countdown {
        if countdown.is_expired() {
            break;
        }
        println!("Countdown: {}", format_remaining(countdown.remaining()));
        std::thread::sleep(Duration::from_secs(1));
    }

    state.mark_expired_checking();
    state.apply_voice_status_data(
        now_wall_clock_ms(),
        VoiceStatusData {
            is_voice_enabled: true,
            is_user_opt_in: true,
            is_user_eligible: true,
            is_banned: false,
            ban_reason: None,
            banned_until_ms: None,
            denial_reason: None,
        },
    );

    let last_server = roblox_logs::detect_last_server_from_logs();
    if matches!(state.voice_state, VoiceState::Restored { .. }) {
        overlay::show_restored_overlay(last_server.as_ref())?;
    }

    Ok(())
}

struct TrayApp {
    state: AppState,
    tray_icon: Option<TrayIcon>,
    status_item: Option<MenuItem>,
    countdown_item: Option<MenuItem>,
    last_checked_item: Option<MenuItem>,
    menu_shape: Option<MenuShape>,
}

impl TrayApp {
    fn new(_settings: settings::Settings) -> Self {
        Self {
            state: AppState::default(),
            tray_icon: None,
            status_item: None,
            countdown_item: None,
            last_checked_item: None,
            menu_shape: None,
        }
    }

    fn build_tray(&mut self) -> Result<()> {
        let shape = self.desired_menu_shape();
        let menu = self.create_menu(shape)?;

        self.tray_icon = Some(
            TrayIconBuilder::new()
                .with_tooltip("Voice Watch")
                .with_icon(make_icon()?)
                .with_menu(Box::new(menu))
                .build()?,
        );
        self.menu_shape = Some(shape);

        self.refresh_menu_labels();
        Ok(())
    }

    fn desired_menu_shape(&self) -> MenuShape {
        MenuShape {
            browser_connected: self.state.is_browser_connected(),
            has_countdown: self.state.countdown.is_some(),
        }
    }

    fn create_menu(&mut self, shape: MenuShape) -> Result<Menu> {
        let menu = Menu::new();

        let title = MenuItem::with_id("title", "Voice Watch", false, None);
        let status = MenuItem::with_id("status", "Status: Disconnected", false, None);
        let last_checked = MenuItem::with_id("last_checked", "Last checked: --", false, None);
        let check_now = MenuItem::with_id("check_now", "Check now", true, None);
        let settings = MenuItem::with_id("settings", "Settings", true, None);
        let quit = MenuItem::with_id("quit", "Quit", true, None);

        menu.append(&title)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&status)?;

        self.countdown_item = None;
        if shape.has_countdown {
            let countdown = MenuItem::with_id("countdown", "Countdown: --", false, None);
            menu.append(&countdown)?;
            self.countdown_item = Some(countdown);
        }

        menu.append(&last_checked)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&check_now)?;

        if !shape.browser_connected {
            let connect = MenuItem::with_id("connect", "Connect Roblox", true, None);
            menu.append(&connect)?;
        }

        menu.append(&settings)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit)?;

        self.status_item = Some(status);
        self.last_checked_item = Some(last_checked);

        Ok(menu)
    }

    fn refresh_tray(&mut self) {
        let desired_shape = self.desired_menu_shape();
        if self.menu_shape != Some(desired_shape) {
            match self.create_menu(desired_shape) {
                Ok(menu) => {
                    if let Some(tray_icon) = &self.tray_icon {
                        tray_icon.set_menu(Some(Box::new(menu)));
                        self.menu_shape = Some(desired_shape);
                    }
                }
                Err(error) => {
                    eprintln!("Failed to update tray menu: {error:#}");
                }
            }
        }

        self.refresh_menu_labels();
    }

    fn refresh_menu_labels(&self) {
        if let Some(item) = &self.status_item {
            item.set_text(format!("Status: {}", self.state.voice_state.label()));
        }

        if let Some(item) = &self.countdown_item {
            let text = self
                .state
                .countdown
                .as_ref()
                .map(|countdown| format_remaining(countdown.remaining()))
                .unwrap_or_else(|| "--".into());
            item.set_text(format!("Countdown: {text}"));
        }

        if let Some(item) = &self.last_checked_item {
            let now_ms = now_wall_clock_ms();
            let text = self
                .state
                .last_checked_at_ms
                .map(|value| format_relative_time(value, now_ms))
                .unwrap_or_else(|| "--".into());
            item.set_text(format!("Last checked: {text}"));
        }
    }

    fn handle_menu(&mut self, event_loop: &ActiveEventLoop, event: MenuEvent) {
        match event.id().0.as_str() {
            "quit" => event_loop.exit(),
            "check_now" => {
                let running = process::is_roblox_running();
                if running {
                    self.state.mark_checking();
                } else {
                    self.state.mark_roblox_not_running();
                }
                self.refresh_tray();
            }
            "connect" => {
                let _ = open_setup_page(self.state.is_browser_connected());
            }
            "settings" => {
                if let Ok(path) = settings::settings_path() {
                    let _ = open::that(path);
                }
            }
            _ => {}
        }
    }

    fn handle_ipc(&mut self, event: IpcEvent) {
        match event {
            IpcEvent::ExtensionConnected { .. } => {
                self.state.mark_connected();
            }
            IpcEvent::ExtensionDisconnected { .. } => {
                self.state.mark_disconnected();
            }
            IpcEvent::VoiceStatus { envelope, .. } => {
                self.state.apply_voice_status(envelope);
            }
        }
        self.refresh_tray();
    }
}

impl ApplicationHandler<UserEvent> for TrayApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray_icon.is_none() {
            if let Err(error) = self.build_tray() {
                eprintln!("Failed to create tray icon: {error:#}");
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(menu_event) => self.handle_menu(event_loop, menu_event),
            UserEvent::Ipc(ipc_event) => self.handle_ipc(ipc_event),
            UserEvent::Tick => self.refresh_tray(),
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

fn open_setup_page(is_connected: bool) -> Result<()> {
    let path = extension_setup_page()?;
    let query = browser_support::setup_query(is_connected);
    let url = format!("{}?{query}", file_url(&path));
    open::that(url).context("failed to open browser connector setup")
}

fn extension_setup_page() -> Result<std::path::PathBuf> {
    let exe_path = std::env::current_exe().context("failed to locate Voice Watch executable")?;
    let app_dir = exe_path
        .parent()
        .context("Voice Watch executable has no parent directory")?;
    let installed_setup = app_dir.join("extension").join("setup.html");
    if installed_setup.exists() {
        return Ok(installed_setup);
    }

    Ok(std::env::current_dir()?
        .join("extension")
        .join("setup.html"))
}

fn file_url(path: &std::path::Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };
    format!("file://{}", encode_file_url_path(&path))
}

fn encode_file_url_path(path: &str) -> String {
    path.bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn format_relative_time(checked_at_ms: i64, now_ms: i64) -> String {
    let elapsed_seconds = now_ms.saturating_sub(checked_at_ms).max(0) / 1000;
    match elapsed_seconds {
        0..=4 => "just now".into(),
        5..=59 => plural(elapsed_seconds, "second"),
        60..=3599 => plural(elapsed_seconds / 60, "minute"),
        3600..=86399 => plural(elapsed_seconds / 3600, "hour"),
        _ => plural(elapsed_seconds / 86400, "day"),
    }
}

fn plural(value: i64, unit: &str) -> String {
    if value == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{value} {unit}s ago")
    }
}

fn make_icon() -> Result<Icon> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let inside = dx * dx + dy * dy <= 15 * 15;
            if inside {
                rgba.extend_from_slice(&[42, 184, 120, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).context("failed to create tray icon")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_is_human_readable() {
        assert_eq!(format_relative_time(10_000, 12_000), "just now");
        assert_eq!(format_relative_time(10_000, 20_000), "10 seconds ago");
        assert_eq!(format_relative_time(10_000, 70_000), "1 minute ago");
        assert_eq!(format_relative_time(10_000, 130_000), "2 minutes ago");
        assert_eq!(format_relative_time(10_000, 3_610_000), "1 hour ago");
    }

    #[test]
    fn file_urls_escape_spaces() {
        assert_eq!(
            file_url(std::path::Path::new(
                r"C:\Program Files\Voice Watch\setup.html"
            )),
            "file:///C:/Program%20Files/Voice%20Watch/setup.html"
        );
    }
}
