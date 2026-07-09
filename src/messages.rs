use crate::rejoin::LastServer;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const NATIVE_HOST_NAME: &str = "com.voice_watch.native";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtensionMessage {
    Hello {
        #[serde(rename = "extensionVersion")]
        extension_version: String,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
    },
    PollReadinessRequest {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    LastServer {
        server: LastServer,
    },
    Disconnect,
    VoiceStatus(VoiceStatusEnvelope),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppMessage {
    HelloAck {
        #[serde(rename = "appVersion")]
        app_version: String,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        #[serde(rename = "pollIntervalSeconds")]
        poll_interval_seconds: u64,
    },
    CheckVoiceStatus {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    PollReadiness {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "shouldPoll")]
        should_poll: bool,
        #[serde(rename = "robloxRunning")]
        roblox_running: bool,
        #[serde(rename = "robloxPlaying")]
        roblox_playing: bool,
        #[serde(rename = "microphoneActive")]
        microphone_active: bool,
        reason: Option<String>,
        message: Option<String>,
    },
    StatusAck {
        #[serde(rename = "requestId")]
        request_id: Option<String>,
        accepted: bool,
    },
    LastServerAck {
        accepted: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceStatusEnvelope {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "checkedAt")]
    pub checked_at: i64,
    pub ok: bool,
    #[serde(default)]
    pub data: Option<VoiceStatusData>,
    #[serde(default)]
    pub error: Option<VoiceStatusError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceStatusData {
    #[serde(rename = "isVoiceEnabled")]
    pub is_voice_enabled: bool,
    #[serde(rename = "isUserOptIn")]
    pub is_user_opt_in: bool,
    #[serde(rename = "isUserEligible")]
    pub is_user_eligible: bool,
    #[serde(rename = "isBanned")]
    pub is_banned: bool,
    #[serde(rename = "banReason")]
    pub ban_reason: Option<i32>,
    #[serde(rename = "bannedUntilMs")]
    pub banned_until_ms: Option<i64>,
    #[serde(rename = "denialReason")]
    pub denial_reason: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceStatusError {
    pub kind: VoiceStatusErrorKind,
    pub message: String,
    #[serde(rename = "retryAfterMs")]
    pub retry_after_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceStatusErrorKind {
    AuthError,
    NetworkError,
    RateLimited,
    UnexpectedResponse,
}
