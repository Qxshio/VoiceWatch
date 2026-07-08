use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastServer {
    pub place_id: Option<u64>,
    pub game_instance_id: Option<String>,
    pub detected_at_ms: i64,
}

impl LastServer {
    pub fn is_actionable(&self) -> bool {
        self.place_id.is_some()
    }
}

pub fn rejoin_target_url(server: &LastServer) -> Option<String> {
    let place_id = server.place_id?;

    match server.game_instance_id.as_deref() {
        Some(game_instance_id) if !game_instance_id.trim().is_empty() => Some(format!(
            "roblox://experiences/start?placeId={place_id}&gameInstanceId={}",
            url_escape(game_instance_id)
        )),
        _ => Some(format!("https://www.roblox.com/games/{place_id}")),
    }
}

pub fn open_rejoin_target(server: &LastServer) -> Result<()> {
    let url = rejoin_target_url(server).ok_or_else(|| anyhow!("last server is unavailable"))?;
    open::that(&url).with_context(|| format!("failed to open rejoin target: {url}"))?;
    Ok(())
}

fn url_escape(input: &str) -> String {
    input
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
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
    fn uses_place_page_when_exact_server_missing() {
        let server = LastServer {
            place_id: Some(123),
            game_instance_id: None,
            detected_at_ms: 0,
        };
        assert_eq!(
            rejoin_target_url(&server).unwrap(),
            "https://www.roblox.com/games/123"
        );
    }
}
