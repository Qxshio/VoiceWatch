use sysinfo::{ProcessRefreshKind, RefreshKind, System};

pub const ROBLOX_PLAYER_PROCESS: &str = "RobloxPlayerBeta.exe";

pub fn is_roblox_running() -> bool {
    let refresh = RefreshKind::new().with_processes(ProcessRefreshKind::new());
    let mut system = System::new_with_specifics(refresh);
    system.refresh_processes();

    system
        .processes()
        .values()
        .any(|process| process.name().eq_ignore_ascii_case(ROBLOX_PLAYER_PROCESS))
}
