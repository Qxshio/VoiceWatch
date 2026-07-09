use crate::app_state::{AppState, VoiceState};
use crate::browser_support;
use crate::countdown::{format_remaining, now_wall_clock_ms};
use crate::ipc::{self, IpcEvent};
use crate::messages::{VoiceStatusData, VoiceStatusEnvelope};
use crate::overlay;
use crate::process;
use crate::rejoin;
use crate::roblox_logs;
use crate::settings;
use crate::settings_window;
use crate::updates::{self, UpdateEvent, UpdateInfo};
use anyhow::{Context, Result};
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

#[derive(Debug, Clone)]
enum UserEvent {
    Menu(MenuEvent),
    Ipc(IpcEvent),
    Update(UpdateEvent),
    Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MenuShape {
    browser_connected: bool,
    has_countdown: bool,
    developer_mode: bool,
    can_test_suspend: bool,
    update: UpdateMenuShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateMenuShape {
    Hidden,
    Available,
    Installing,
    Failed,
}

#[derive(Debug, Clone)]
enum UpdateTrayState {
    Idle,
    Available(UpdateInfo),
    Installing(UpdateInfo),
    Failed { info: UpdateInfo, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayIconStyle {
    Normal,
    UpdateAvailable,
    Updating,
}

const TEST_SUSPENSION_SECONDS: i64 = 120;
const UPDATE_CHECK_INITIAL_DELAY: Duration = Duration::from_secs(20);
const UPDATE_CHECK_RETRY_INTERVAL: Duration = Duration::from_secs(2 * 60);
const UPDATE_CHECK_FAST_RETRY_LIMIT: u8 = 5;
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

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

    let update_proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        std::thread::sleep(UPDATE_CHECK_INITIAL_DELAY);
        let mut consecutive_misses = 0;
        loop {
            let found_update = match updates::check_for_update() {
                Ok(Some(info)) => {
                    let _ =
                        update_proxy.send_event(UserEvent::Update(UpdateEvent::Available(info)));
                    true
                }
                Ok(None) => false,
                Err(error) => {
                    eprintln!("Update check failed: {error:#}");
                    false
                }
            };
            std::thread::sleep(next_update_check_delay(
                found_update,
                &mut consecutive_misses,
            ));
        }
    });

    let app_proxy = event_loop.create_proxy();
    let mut app = TrayApp::new(settings, app_proxy);
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

    state.mark_restored(now_wall_clock_ms());

    let last_server = roblox_logs::detect_last_server_from_logs();
    if matches!(state.voice_state, VoiceState::Restored { .. }) {
        overlay::play_restore_sound();
        overlay::show_restored_overlay(last_server.as_ref())?;
    }

    Ok(())
}

struct TrayApp {
    proxy: EventLoopProxy<UserEvent>,
    settings: settings::Settings,
    state: AppState,
    update_state: UpdateTrayState,
    hud: overlay::SuspensionHud,
    last_server: Option<rejoin::LastServer>,
    test_suspension_until_ms: Option<i64>,
    setup_opened_on_startup: bool,
    tray_icon: Option<TrayIcon>,
    tray_icon_style: Option<TrayIconStyle>,
    update_item: Option<MenuItem>,
    status_item: Option<MenuItem>,
    countdown_item: Option<MenuItem>,
    last_checked_item: Option<MenuItem>,
    menu_shape: Option<MenuShape>,
}

impl TrayApp {
    fn new(settings: settings::Settings, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            proxy,
            settings,
            state: AppState::default(),
            update_state: UpdateTrayState::Idle,
            hud: overlay::SuspensionHud::default(),
            last_server: ipc::read_last_server(),
            test_suspension_until_ms: None,
            setup_opened_on_startup: false,
            tray_icon: None,
            tray_icon_style: None,
            update_item: None,
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
                .with_icon(make_icon(TrayIconStyle::Normal)?)
                .with_menu(Box::new(menu))
                .build()?,
        );
        self.menu_shape = Some(shape);

        self.refresh_menu_labels();
        self.refresh_hud();
        Ok(())
    }

    fn desired_menu_shape(&self) -> MenuShape {
        MenuShape {
            browser_connected: self.state.is_browser_connected(),
            has_countdown: self.state.countdown.is_some(),
            developer_mode: self.settings.developer_mode,
            can_test_suspend: self.can_test_suspend(),
            update: self.update_state.menu_shape(),
        }
    }

    fn can_test_suspend(&self) -> bool {
        !matches!(
            self.state.voice_state,
            VoiceState::TempSuspended { .. } | VoiceState::SuspendedUnknownDuration { .. }
        )
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

        self.update_item = None;
        if shape.update != UpdateMenuShape::Hidden {
            let update = MenuItem::with_id(
                "update",
                self.update_state.menu_label(),
                shape.update != UpdateMenuShape::Installing,
                None,
            );
            menu.append(&update)?;
            menu.append(&PredefinedMenuItem::separator())?;
            self.update_item = Some(update);
        }

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

        if shape.developer_mode {
            menu.append(&PredefinedMenuItem::separator())?;
            let test_suspend =
                MenuItem::with_id("test_suspend", "Test Suspend", shape.can_test_suspend, None);
            menu.append(&test_suspend)?;
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
        self.refresh_tray_icon();
        self.refresh_hud();
    }

    fn refresh_menu_labels(&self) {
        if let Some(item) = &self.update_item {
            item.set_text(self.update_state.menu_label());
            item.set_enabled(!matches!(self.update_state, UpdateTrayState::Installing(_)));
        }

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
            "test_suspend" => {
                if self.can_test_suspend() {
                    let now_ms = now_wall_clock_ms();
                    let banned_until_ms = now_ms + TEST_SUSPENSION_SECONDS * 1000;
                    if let Some(server) = roblox_logs::detect_last_server_from_logs() {
                        self.remember_last_server(server);
                    }
                    self.test_suspension_until_ms = Some(banned_until_ms);
                    self.state.mark_test_suspended(now_ms, banned_until_ms);
                    self.refresh_tray();
                }
            }
            "settings" => {
                let _ = settings_window::open();
            }
            "update" => self.start_update_install(),
            _ => {}
        }
    }

    fn handle_ipc(&mut self, event: IpcEvent) {
        match event {
            IpcEvent::ExtensionConnected { .. } => {
                self.state.mark_connected();
            }
            IpcEvent::ExtensionDisconnected { .. } => {
                self.test_suspension_until_ms = None;
                self.state.mark_disconnected();
            }
            IpcEvent::LastServer { server, .. } => {
                self.remember_last_server(server);
            }
            IpcEvent::VoiceStatus { envelope, .. } => {
                if should_ignore_voice_status_for_test_suspend(
                    self.test_suspension_until_ms,
                    &envelope,
                    now_wall_clock_ms(),
                ) {
                    self.state.mark_connected();
                    self.refresh_tray();
                    return;
                }
                self.test_suspension_until_ms = None;
                let was_restored = matches!(self.state.voice_state, VoiceState::Restored { .. });
                self.state.apply_voice_status(envelope);
                if matches!(self.state.voice_state, VoiceState::Restored { .. }) && !was_restored {
                    self.announce_restored();
                }
            }
        }
        self.refresh_tray();
    }

    fn handle_update(&mut self, event_loop: &ActiveEventLoop, event: UpdateEvent) {
        match event {
            UpdateEvent::Available(info) => {
                if self.update_state.should_accept_available(&info) {
                    self.update_state = UpdateTrayState::Available(info);
                }
            }
            UpdateEvent::InstallLaunched => {
                event_loop.exit();
                return;
            }
            UpdateEvent::InstallFailed { info, message } => {
                self.update_state = UpdateTrayState::Failed { info, message };
            }
        }
        self.refresh_tray();
    }

    fn handle_tick(&mut self) {
        self.reload_settings();

        if matches!(self.state.voice_state, VoiceState::TempSuspended { .. })
            && self
                .state
                .countdown
                .as_ref()
                .is_some_and(|countdown| countdown.is_expired())
        {
            self.test_suspension_until_ms = None;
            self.state.mark_restored(now_wall_clock_ms());
            self.announce_restored();
        }

        if !self.setup_opened_on_startup && !self.state.is_browser_connected() {
            self.setup_opened_on_startup = true;
            let _ = open_setup_page(false);
        }

        self.refresh_tray();
    }

    fn start_update_install(&mut self) {
        let Some(info) = self.update_state.update_info().cloned() else {
            return;
        };

        self.update_state = UpdateTrayState::Installing(info.clone());
        self.refresh_tray();

        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            match updates::download_and_launch_update(&info) {
                Ok(()) => {
                    let _ = proxy.send_event(UserEvent::Update(UpdateEvent::InstallLaunched));
                    std::thread::sleep(Duration::from_millis(250));
                    std::process::exit(0);
                }
                Err(error) => {
                    let event = UpdateEvent::InstallFailed {
                        info,
                        message: error.to_string(),
                    };
                    let _ = proxy.send_event(UserEvent::Update(event));
                }
            };
        });
    }

    fn refresh_tray_icon(&mut self) {
        let style = self.update_state.icon_style();
        if self.tray_icon_style == Some(style) {
            return;
        }

        if let Some(tray_icon) = &self.tray_icon {
            if let Ok(icon) = make_icon(style) {
                let _ = tray_icon.set_icon(Some(icon));
            }
            let tooltip = match &self.update_state {
                UpdateTrayState::Available(info) => {
                    format!("Voice Watch - Update {} available", info.version)
                }
                UpdateTrayState::Installing(info) => {
                    format!("Voice Watch - Installing {}", info.version)
                }
                UpdateTrayState::Failed { message, .. } => {
                    format!("Voice Watch - Update failed: {message}")
                }
                UpdateTrayState::Idle => "Voice Watch".into(),
            };
            let _ = tray_icon.set_tooltip(Some(tooltip));
        }

        self.tray_icon_style = Some(style);
    }

    fn reload_settings(&mut self) {
        if let Ok(settings) = settings::load_settings() {
            self.settings = settings;
        }
    }

    fn announce_restored(&mut self) {
        if let Some(server) = roblox_logs::detect_last_server_from_logs() {
            self.remember_last_server(server);
        }

        if self.state.restored_overlay_shown {
            return;
        }

        if self.settings.play_sound_on_restore {
            overlay::play_restore_sound();
        }
        self.state.restored_overlay_shown = true;
    }

    fn remember_last_server(&mut self, server: rejoin::LastServer) {
        let should_replace = match &self.last_server {
            Some(current) if current.can_rejoin_exact() && !server.can_rejoin_exact() => false,
            Some(current) => server.detected_at_ms >= current.detected_at_ms,
            None => true,
        };

        if should_replace {
            self.last_server = Some(server);
        }
    }

    fn refresh_hud(&mut self) {
        if !self.settings.show_overlay {
            self.hud.hide();
            return;
        }

        match &self.state.voice_state {
            VoiceState::TempSuspended { .. } => {
                let remaining = self
                    .state
                    .countdown
                    .as_ref()
                    .map(|countdown| format_remaining(countdown.remaining()))
                    .unwrap_or_else(|| "--".into());
                self.hud.show_suspended(remaining);
            }
            VoiceState::SuspendedUnknownDuration { .. } => {
                self.hud.show_suspended("unknown".into());
            }
            VoiceState::Restored { .. } => {
                self.hud.show_restored(self.last_server.clone());
            }
            _ => self.hud.hide(),
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
            UserEvent::Ipc(ipc_event) => self.handle_ipc(ipc_event),
            UserEvent::Update(update_event) => self.handle_update(event_loop, update_event),
            UserEvent::Tick => self.handle_tick(),
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

fn should_ignore_voice_status_for_test_suspend(
    test_suspension_until_ms: Option<i64>,
    envelope: &VoiceStatusEnvelope,
    now_ms: i64,
) -> bool {
    let Some(test_suspension_until_ms) = test_suspension_until_ms else {
        return false;
    };

    if now_ms >= test_suspension_until_ms {
        return false;
    }

    envelope
        .data
        .as_ref()
        .is_some_and(|data| envelope.ok && !data.is_banned)
}

fn plural(value: i64, unit: &str) -> String {
    if value == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{value} {unit}s ago")
    }
}

fn next_update_check_delay(found_update: bool, consecutive_misses: &mut u8) -> Duration {
    if found_update {
        *consecutive_misses = 0;
        return UPDATE_CHECK_INTERVAL;
    }

    if *consecutive_misses < UPDATE_CHECK_FAST_RETRY_LIMIT {
        *consecutive_misses += 1;
        UPDATE_CHECK_RETRY_INTERVAL
    } else {
        UPDATE_CHECK_INTERVAL
    }
}

impl UpdateTrayState {
    fn menu_shape(&self) -> UpdateMenuShape {
        match self {
            Self::Idle => UpdateMenuShape::Hidden,
            Self::Available(_) => UpdateMenuShape::Available,
            Self::Installing(_) => UpdateMenuShape::Installing,
            Self::Failed { .. } => UpdateMenuShape::Failed,
        }
    }

    fn update_info(&self) -> Option<&UpdateInfo> {
        match self {
            Self::Available(info) | Self::Installing(info) => Some(info),
            Self::Failed { info, .. } => Some(info),
            Self::Idle => None,
        }
    }

    fn should_accept_available(&self, info: &UpdateInfo) -> bool {
        match self {
            Self::Installing(_) => false,
            Self::Available(current) | Self::Failed { info: current, .. } => {
                current.version != info.version
            }
            Self::Idle => true,
        }
    }

    fn menu_label(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Available(info) => format!("Update Available - v{}", info.version),
            Self::Installing(info) => format!("Installing update v{}...", info.version),
            Self::Failed { info, .. } => format!("Update failed - retry v{}", info.version),
        }
    }

    fn icon_style(&self) -> TrayIconStyle {
        match self {
            Self::Idle => TrayIconStyle::Normal,
            Self::Available(_) | Self::Failed { .. } => TrayIconStyle::UpdateAvailable,
            Self::Installing(_) => TrayIconStyle::Updating,
        }
    }
}

fn make_icon(style: TrayIconStyle) -> Result<Icon> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let fill = match style {
        TrayIconStyle::Normal => [42, 184, 120, 255],
        TrayIconStyle::UpdateAvailable => [245, 168, 48, 255],
        TrayIconStyle::Updating => [78, 154, 245, 255],
    };

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let inside = dx * dx + dy * dy <= 15 * 15;
            if inside {
                let marker = matches!(style, TrayIconStyle::UpdateAvailable)
                    && (((13..=18).contains(&x) && (7..=22).contains(&y))
                        || ((9..=22).contains(&x) && (7..=12).contains(&y)));
                if marker {
                    rgba.extend_from_slice(&[255, 255, 255, 255]);
                } else {
                    rgba.extend_from_slice(&fill);
                }
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

    #[test]
    fn test_suspend_ignores_restored_status_until_timer_finishes() {
        let restored = voice_status(false);
        let suspended = voice_status(true);

        assert!(should_ignore_voice_status_for_test_suspend(
            Some(20_000),
            &restored,
            10_000
        ));
        assert!(!should_ignore_voice_status_for_test_suspend(
            Some(20_000),
            &restored,
            20_000
        ));
        assert!(!should_ignore_voice_status_for_test_suspend(
            Some(20_000),
            &suspended,
            10_000
        ));
        assert!(!should_ignore_voice_status_for_test_suspend(
            None, &restored, 10_000
        ));
    }

    #[test]
    fn update_checks_retry_quickly_before_returning_to_normal_cadence() {
        let mut misses = 0;
        for expected_misses in 1..=UPDATE_CHECK_FAST_RETRY_LIMIT {
            assert_eq!(
                next_update_check_delay(false, &mut misses),
                UPDATE_CHECK_RETRY_INTERVAL
            );
            assert_eq!(misses, expected_misses);
        }

        assert_eq!(
            next_update_check_delay(false, &mut misses),
            UPDATE_CHECK_INTERVAL
        );

        assert_eq!(
            next_update_check_delay(true, &mut misses),
            UPDATE_CHECK_INTERVAL
        );
        assert_eq!(misses, 0);
    }

    fn voice_status(is_banned: bool) -> VoiceStatusEnvelope {
        VoiceStatusEnvelope {
            request_id: "test".into(),
            checked_at: 1_000,
            ok: true,
            data: Some(VoiceStatusData {
                is_voice_enabled: !is_banned,
                is_user_opt_in: true,
                is_user_eligible: true,
                is_banned,
                ban_reason: if is_banned { Some(7) } else { None },
                banned_until_ms: if is_banned { Some(20_000) } else { None },
                denial_reason: if is_banned { Some(6) } else { None },
            }),
            error: None,
        }
    }
}
