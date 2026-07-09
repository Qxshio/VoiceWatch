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
            && (valid_game_instance_id(self.game_instance_id.as_deref()).is_some()
                || non_empty(self.access_code.as_deref()).is_some()
                || non_empty(self.link_code.as_deref()).is_some())
    }
}

pub fn rejoin_target_url(server: &LastServer) -> Option<String> {
    let place_id = server.place_id?;

    let mut params = vec![
        "voiceWatchRejoin=1".to_string(),
        format!("placeId={place_id}"),
    ];
    if let Some(game_instance_id) = valid_game_instance_id(server.game_instance_id.as_deref()) {
        params.push(format!("gameInstanceId={}", url_escape(game_instance_id)));
    }
    if let Some(access_code) = non_empty(server.access_code.as_deref()) {
        params.push(format!("accessCode={}", url_escape(access_code)));
    }
    if let Some(link_code) = non_empty(server.link_code.as_deref()) {
        params.push(format!("linkCode={}", url_escape(link_code)));
    }

    Some(format!(
        "https://www.roblox.com/games/{place_id}/Voice-Watch?{}",
        params.join("&")
    ))
}

pub fn roblox_deep_link_url(server: &LastServer) -> Option<String> {
    let place_id = server.place_id?;

    let mut params = vec![format!("placeId={place_id}")];
    if let Some(game_instance_id) = valid_game_instance_id(server.game_instance_id.as_deref()) {
        params.push(format!("gameInstanceId={}", url_escape(game_instance_id)));
    }
    if let Some(access_code) = non_empty(server.access_code.as_deref()) {
        params.push(format!("accessCode={}", url_escape(access_code)));
    }
    if let Some(link_code) = non_empty(server.link_code.as_deref()) {
        params.push(format!("linkCode={}", url_escape(link_code)));
    }

    Some(format!("roblox://{}", params.join("&")))
}

pub fn open_rejoin_target(server: &LastServer) -> Result<()> {
    if !server.can_rejoin_exact() {
        return Err(anyhow!("exact last server is unavailable"));
    }

    let direct_url =
        roblox_deep_link_url(server).ok_or_else(|| anyhow!("last server is unavailable"))?;
    match open::that(&direct_url) {
        Ok(()) => return Ok(()),
        Err(direct_error) => {
            let fallback_url =
                rejoin_target_url(server).ok_or_else(|| anyhow!("last server is unavailable"))?;
            open::that(&fallback_url).with_context(|| {
                format!(
                    "failed to open Roblox app link {direct_url} and browser fallback {fallback_url}; app link error: {direct_error:#}"
                )
            })?;
        }
    }

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

fn valid_game_instance_id(value: Option<&str>) -> Option<&str> {
    let value = non_empty(value)?;
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return None;
    }

    for (index, byte) in bytes.iter().enumerate() {
        let is_hyphen = matches!(index, 8 | 13 | 18 | 23);
        if is_hyphen {
            if *byte != b'-' {
                return None;
            }
        } else if !byte.is_ascii_hexdigit() {
            return None;
        }
    }

    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_place_page_marker_when_exact_server_missing() {
        let server = LastServer {
            place_id: Some(123),
            game_instance_id: None,
            access_code: None,
            link_code: None,
            detected_at_ms: 0,
        };
        assert_eq!(
            rejoin_target_url(&server).unwrap(),
            "https://www.roblox.com/games/123/Voice-Watch?voiceWatchRejoin=1&placeId=123"
        );
    }

    #[test]
    fn uses_game_instance_for_exact_public_server() {
        let server = LastServer {
            place_id: Some(123),
            game_instance_id: Some("1bb8dd1d-ad4c-43d2-a9c6-63feee836e43".into()),
            access_code: None,
            link_code: None,
            detected_at_ms: 0,
        };

        assert!(server.can_rejoin_exact());
        assert_eq!(
            rejoin_target_url(&server).unwrap(),
            "https://www.roblox.com/games/123/Voice-Watch?voiceWatchRejoin=1&placeId=123&gameInstanceId=1bb8dd1d-ad4c-43d2-a9c6-63feee836e43"
        );
        assert_eq!(
            roblox_deep_link_url(&server).unwrap(),
            "roblox://placeId=123&gameInstanceId=1bb8dd1d-ad4c-43d2-a9c6-63feee836e43"
        );
    }

    #[test]
    fn rejects_non_job_id_game_instance_values() {
        let server = LastServer {
            place_id: Some(123),
            game_instance_id: Some("false".into()),
            access_code: None,
            link_code: None,
            detected_at_ms: 0,
        };

        assert!(!server.can_rejoin_exact());
        assert_eq!(
            rejoin_target_url(&server).unwrap(),
            "https://www.roblox.com/games/123/Voice-Watch?voiceWatchRejoin=1&placeId=123"
        );
        assert_eq!(
            roblox_deep_link_url(&server).unwrap(),
            "roblox://placeId=123"
        );
    }

    #[test]
    fn includes_private_server_access_code_with_marker() {
        let server = LastServer {
            place_id: Some(123),
            game_instance_id: Some("1bb8dd1d-ad4c-43d2-a9c6-63feee836e43".into()),
            access_code: Some("private code".into()),
            link_code: None,
            detected_at_ms: 0,
        };

        assert_eq!(
            rejoin_target_url(&server).unwrap(),
            "https://www.roblox.com/games/123/Voice-Watch?voiceWatchRejoin=1&placeId=123&gameInstanceId=1bb8dd1d-ad4c-43d2-a9c6-63feee836e43&accessCode=private%20code"
        );
        assert_eq!(
            roblox_deep_link_url(&server).unwrap(),
            "roblox://placeId=123&gameInstanceId=1bb8dd1d-ad4c-43d2-a9c6-63feee836e43&accessCode=private%20code"
        );
    }
}
