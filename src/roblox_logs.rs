use crate::countdown::now_wall_clock_ms;
use crate::rejoin::LastServer;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static PLACE_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:placeId|place_id|placeid)(?:"|%22)?\s*(?::|=|%3a|\s)+\s*(?:"|%22)?(\d+)"#)
        .unwrap()
});
static JOB_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:gameInstanceId|jobId|game_instance_id|gameId)(?:"|%22)?\s*(?::|=|%3a|\s)+\s*(?:"|%22)?([A-Za-z0-9\-]+)"#).unwrap()
});
static ACCESS_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:accessCode|reservedServerAccessCode)(?:"|%22)?\s*(?::|=|%3a|\s)+\s*(?:"|%22)?([A-Za-z0-9\-]+)"#).unwrap()
});
static LINK_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)linkCode(?:"|%22)?\s*(?::|=|%3a|\s)+\s*(?:"|%22)?([A-Za-z0-9\-]+)"#).unwrap()
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
    Ok(detect_last_server_in_text(&contents))
}

fn detect_last_server_in_text(contents: &str) -> Option<LastServer> {
    let mut fallback = None;

    for server in contents.lines().rev().filter_map(parse_last_server_line) {
        if server.can_rejoin_exact() {
            return Some(server);
        }
        fallback.get_or_insert(server);
    }

    fallback
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
    let access_code = ACCESS_CODE
        .captures(line)
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_string());
    let link_code = LINK_CODE
        .captures(line)
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_string());

    if place_id.is_none()
        && game_instance_id.is_none()
        && access_code.is_none()
        && link_code.is_none()
    {
        return None;
    }

    Some(LastServer {
        place_id,
        game_instance_id,
        access_code,
        link_code,
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

    #[test]
    fn parses_url_encoded_ticket_game_id() {
        let parsed = parse_last_server_line(
            r#"joinScriptUrl https://assetgame.roblox.com/Game/Join.ashx?ticket={"GameId"%3a"1bb8dd1d-ad4c-43d2-a9c6-63feee836e43"%2c"PlaceId"%3a128736949265057}"#,
        )
        .unwrap();

        assert_eq!(parsed.place_id, Some(128736949265057));
        assert_eq!(
            parsed.game_instance_id.as_deref(),
            Some("1bb8dd1d-ad4c-43d2-a9c6-63feee836e43")
        );
    }

    #[test]
    fn parses_private_server_access_code() {
        let parsed = parse_last_server_line(
            r#"launchGame request {"requestType":"RequestPrivateGame","placeId":"128736949265057","accessCode":"9afa8889-5daf-4853-8933-3b1d5d86c756"}"#,
        )
        .unwrap();

        assert_eq!(parsed.place_id, Some(128736949265057));
        assert_eq!(
            parsed.access_code.as_deref(),
            Some("9afa8889-5daf-4853-8933-3b1d5d86c756")
        );
    }

    #[test]
    fn prefers_exact_server_over_newer_place_only_line() {
        let parsed = detect_last_server_in_text(
            r#"
older joinScriptUrl {"GameId"%3a"1bb8dd1d-ad4c-43d2-a9c6-63feee836e43"%2c"PlaceId"%3a128736949265057}
newer Report game_join_loadtime: placeid:128736949265057
"#,
        )
        .unwrap();

        assert_eq!(
            parsed.game_instance_id.as_deref(),
            Some("1bb8dd1d-ad4c-43d2-a9c6-63feee836e43")
        );
    }
}
