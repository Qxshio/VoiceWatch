use crate::app_state::{AppState, VoiceState};
use crate::countdown::{format_remaining, now_wall_clock_ms};
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
}

pub fn run_tray_app() -> Result<()> {
    let settings = settings::load_settings().context("failed to load settings")?;
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

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
}

impl TrayApp {
    fn new(_settings: settings::Settings) -> Self {
        Self {
            state: AppState::default(),
            tray_icon: None,
            status_item: None,
            countdown_item: None,
            last_checked_item: None,
        }
    }

    fn build_tray(&mut self) -> Result<()> {
        let menu = Menu::new();

        let title = MenuItem::with_id("title", "Voice Watch", false, None);
        let status = MenuItem::with_id("status", "Status: Disconnected", false, None);
        let countdown = MenuItem::with_id("countdown", "Countdown: --", false, None);
        let last_checked = MenuItem::with_id("last_checked", "Last checked: --", false, None);
        let check_now = MenuItem::with_id("check_now", "Check now", true, None);
        let connect = MenuItem::with_id("connect", "Connect Roblox", true, None);
        let settings = MenuItem::with_id("settings", "Settings", true, None);
        let quit = MenuItem::with_id("quit", "Quit", true, None);

        menu.append_items(&[
            &title,
            &PredefinedMenuItem::separator(),
            &status,
            &countdown,
            &last_checked,
            &PredefinedMenuItem::separator(),
            &check_now,
            &connect,
            &settings,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;

        self.status_item = Some(status);
        self.countdown_item = Some(countdown);
        self.last_checked_item = Some(last_checked);

        self.tray_icon = Some(
            TrayIconBuilder::new()
                .with_tooltip("Voice Watch")
                .with_icon(make_icon()?)
                .with_menu(Box::new(menu))
                .build()?,
        );

        self.refresh_menu_labels();
        Ok(())
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
            let text = self
                .state
                .last_checked_at_ms
                .map(|value| value.to_string())
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
                self.refresh_menu_labels();
            }
            "connect" => {
                let _ = open::that("extension/connect.html");
            }
            "settings" => {
                if let Ok(path) = settings::settings_path() {
                    let _ = open::that(path);
                }
            }
            _ => {}
        }
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
