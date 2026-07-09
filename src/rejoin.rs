use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastServer {
    pub place_id: Option<u64>,
    pub game_instance_id: Option<String>,
    #[serde(default)]
    pub access_code: Option<String>,
    #[serde(default)]
    pub link_code: Option<String>,
    pub detected_at_ms: i64,
}

impl LastServer {
    pub fn can_rejoin_exact(&self) -> bool {
        self.place_id.is_some()
            && [
                self.game_instance_id.as_deref(),
                self.access_code.as_deref(),
                self.link_code.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|value| !value.trim().is_empty())
    }
}

pub fn rejoin_target_url(server: &LastServer) -> Option<String> {
    let place_id = server.place_id?;

    let mut params = vec![format!("placeId={place_id}")];
    if let Some(access_code) = non_empty(server.access_code.as_deref()) {
        params.push(format!("accessCode={}", url_escape(access_code)));
    } else if let Some(link_code) = non_empty(server.link_code.as_deref()) {
        params.push(format!("linkCode={}", url_escape(link_code)));
    } else if let Some(game_instance_id) = non_empty(server.game_instance_id.as_deref()) {
        params.push(format!("gameInstanceId={}", url_escape(game_instance_id)));
    }

    Some(format!(
        "https://www.roblox.com/games/start?{}",
        params.join("&")
    ))
}

pub fn open_rejoin_target(server: &LastServer) -> Result<()> {
    if !server.can_rejoin_exact() {
        return Err(anyhow!("exact last server is unavailable"));
    }

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

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_place_start_url_when_exact_server_missing() {
        let server = LastServer {
            place_id: Some(123),
            game_instance_id: None,
            access_code: None,
            link_code: None,
            detected_at_ms: 0,
        };
        assert_eq!(
            rejoin_target_url(&server).unwrap(),
            "https://www.roblox.com/games/start?placeId=123"
        );
    }

    #[test]
    fn uses_game_instance_for_exact_public_server() {
        let server = LastServer {
            place_id: Some(123),
            game_instance_id: Some("abc-def".into()),
            access_code: None,
            link_code: None,
            detected_at_ms: 0,
        };

        assert!(server.can_rejoin_exact());
        assert_eq!(
            rejoin_target_url(&server).unwrap(),
            "https://www.roblox.com/games/start?placeId=123&gameInstanceId=abc-def"
        );
    }

    #[test]
    fn uses_private_server_access_code_before_game_instance() {
        let server = LastServer {
            place_id: Some(123),
            game_instance_id: Some("abc-def".into()),
            access_code: Some("private code".into()),
            link_code: None,
            detected_at_ms: 0,
        };

        assert_eq!(
            rejoin_target_url(&server).unwrap(),
            "https://www.roblox.com/games/start?placeId=123&accessCode=private%20code"
        );
    }
}
