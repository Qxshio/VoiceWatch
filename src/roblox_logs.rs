use crate::countdown::now_wall_clock_ms;
use crate::rejoin::LastServer;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static PLACE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)(?:placeId|place_id)[":=\s]+(\d+)"#).unwrap());
static JOB_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:gameInstanceId|jobId|game_instance_id)[":=\s]+([A-Za-z0-9\-]+)"#).unwrap()
});

pub fn recent_log_paths() -> Vec<PathBuf> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };

    let logs_dir = PathBuf::from(local_app_data).join("Roblox").join("logs");
    let Ok(entries) = fs::read_dir(logs_dir) else {
        return Vec::new();
    };

    let mut files = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((entry.path(), metadata.modified().ok()))
        })
        .collect::<Vec<_>>();

    files.sort_by_key(|(_, modified)| *modified);
    files
        .into_iter()
        .rev()
        .take(5)
        .map(|(path, _)| path)
        .collect()
}

pub fn detect_last_server_from_logs() -> Option<LastServer> {
    recent_log_paths()
        .into_iter()
        .find_map(|path| detect_last_server_in_file(&path).ok().flatten())
}

pub fn detect_last_server_in_file(path: &Path) -> std::io::Result<Option<LastServer>> {
    let contents = fs::read_to_string(path)?;
    Ok(contents.lines().rev().find_map(parse_last_server_line))
}

pub fn parse_last_server_line(line: &str) -> Option<LastServer> {
    let place_id = PLACE_ID
        .captures(line)
        .and_then(|captures| captures.get(1))
        .and_then(|match_| match_.as_str().parse::<u64>().ok());
    let game_instance_id = JOB_ID
        .captures(line)
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_string());

    if place_id.is_none() && game_instance_id.is_none() {
        return None;
    }

    Some(LastServer {
        place_id,
        game_instance_id,
        detected_at_ms: now_wall_clock_ms(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_place_and_job_id_from_log_line() {
        let parsed =
            parse_last_server_line("joining placeId: 12345 gameInstanceId: abc-def").unwrap();
        assert_eq!(parsed.place_id, Some(12345));
        assert_eq!(parsed.game_instance_id.as_deref(), Some("abc-def"));
    }
}
