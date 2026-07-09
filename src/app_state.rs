#![allow(dead_code)]

use crate::countdown::AnchoredCountdown;
use crate::messages::{VoiceStatusData, VoiceStatusEnvelope, VoiceStatusErrorKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceState {
    Disconnected,
    Connected,
    RobloxNotRunning,
    Checking,
    VoiceOk {
        checked_at_ms: i64,
    },
    TempSuspended {
        checked_at_ms: i64,
        banned_until_ms: i64,
        ban_reason: Option<i32>,
        denial_reason: Option<i32>,
    },
    SuspendedUnknownDuration {
        checked_at_ms: i64,
        ban_reason: Option<i32>,
        denial_reason: Option<i32>,
    },
    Ineligible {
        checked_at_ms: i64,
        denial_reason: Option<i32>,
    },
    AuthError {
        checked_at_ms: i64,
    },
    NetworkError {
        checked_at_ms: i64,
        message: String,
    },
    RateLimited {
        checked_at_ms: i64,
        retry_after_ms: Option<i64>,
    },
    ExpiredChecking,
    Restored {
        checked_at_ms: i64,
    },
}

impl VoiceState {
    pub fn label(&self) -> String {
        match self {
            VoiceState::Disconnected => "Disconnected".into(),
            VoiceState::Connected => "Connected".into(),
            VoiceState::RobloxNotRunning => "Roblox not in game".into(),
            VoiceState::Checking => "Checking voice status".into(),
            VoiceState::VoiceOk { .. } => "Voice chat available".into(),
            VoiceState::TempSuspended { .. } => "Voice chat suspended".into(),
            VoiceState::SuspendedUnknownDuration { .. } => "Suspended, duration unknown".into(),
            VoiceState::Ineligible { .. } => "Voice chat unavailable".into(),
            VoiceState::AuthError { .. } => "Browser session not connected".into(),
            VoiceState::NetworkError { .. } => "Network error".into(),
            VoiceState::RateLimited { .. } => "Rate limited".into(),
            VoiceState::ExpiredChecking => "Timer expired, confirming".into(),
            VoiceState::Restored { .. } => "Voice chat restored".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub voice_state: VoiceState,
    pub browser_connected: bool,
    pub countdown: Option<AnchoredCountdown>,
    pub last_checked_at_ms: Option<i64>,
    pub restored_overlay_shown: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            voice_state: VoiceState::Disconnected,
            browser_connected: false,
            countdown: None,
            last_checked_at_ms: None,
            restored_overlay_shown: false,
        }
    }
}

impl AppState {
    pub fn mark_connected(&mut self) {
        self.browser_connected = true;
        if matches!(self.voice_state, VoiceState::Disconnected) {
            self.voice_state = VoiceState::Connected;
        }
    }

    pub fn mark_disconnected(&mut self) {
        self.browser_connected = false;
        self.voice_state = VoiceState::Disconnected;
        self.countdown = None;
    }

    pub fn is_browser_connected(&self) -> bool {
        self.browser_connected
    }

    pub fn mark_checking(&mut self) {
        self.voice_state = VoiceState::Checking;
    }

    pub fn mark_roblox_not_running(&mut self) {
        self.voice_state = VoiceState::RobloxNotRunning;
    }

    pub fn mark_expired_checking(&mut self) {
        self.voice_state = VoiceState::ExpiredChecking;
    }

    pub fn apply_voice_status(&mut self, envelope: VoiceStatusEnvelope) {
        self.browser_connected = true;
        self.last_checked_at_ms = Some(envelope.checked_at);

        if !envelope.ok {
            self.countdown = None;
            let Some(error) = envelope.error else {
                self.voice_state = VoiceState::NetworkError {
                    checked_at_ms: envelope.checked_at,
                    message: "Status check failed without an error body".into(),
                };
                return;
            };

            self.voice_state = match error.kind {
                VoiceStatusErrorKind::AuthError => VoiceState::AuthError {
                    checked_at_ms: envelope.checked_at,
                },
                VoiceStatusErrorKind::RateLimited => VoiceState::RateLimited {
                    checked_at_ms: envelope.checked_at,
                    retry_after_ms: error.retry_after_ms,
                },
                VoiceStatusErrorKind::NetworkError | VoiceStatusErrorKind::UnexpectedResponse => {
                    VoiceState::NetworkError {
                        checked_at_ms: envelope.checked_at,
                        message: error.message,
                    }
                }
            };
            return;
        }

        let Some(data) = envelope.data else {
            self.countdown = None;
            self.voice_state = VoiceState::NetworkError {
                checked_at_ms: envelope.checked_at,
                message: "Status check succeeded without sanitized data".into(),
            };
            return;
        };

        self.apply_voice_status_data(envelope.checked_at, data);
    }

    pub fn apply_voice_status_data(&mut self, checked_at_ms: i64, data: VoiceStatusData) {
        self.browser_connected = true;
        self.last_checked_at_ms = Some(checked_at_ms);

        if data.is_banned {
            self.restored_overlay_shown = false;
            match data.banned_until_ms {
                Some(banned_until_ms) => {
                    self.countdown = Some(AnchoredCountdown::new(banned_until_ms));
                    self.voice_state = VoiceState::TempSuspended {
                        checked_at_ms,
                        banned_until_ms,
                        ban_reason: data.ban_reason,
                        denial_reason: data.denial_reason,
                    };
                }
                None => {
                    self.countdown = None;
                    self.voice_state = VoiceState::SuspendedUnknownDuration {
                        checked_at_ms,
                        ban_reason: data.ban_reason,
                        denial_reason: data.denial_reason,
                    };
                }
            }
            return;
        }

        self.countdown = None;
        if data.is_voice_enabled && data.is_user_opt_in && data.is_user_eligible {
            self.voice_state = if matches!(self.voice_state, VoiceState::ExpiredChecking) {
                VoiceState::Restored { checked_at_ms }
            } else {
                VoiceState::VoiceOk { checked_at_ms }
            };
        } else {
            self.voice_state = VoiceState::Ineligible {
                checked_at_ms,
                denial_reason: data.denial_reason,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banned_status_starts_countdown() {
        let mut state = AppState::default();
        state.apply_voice_status_data(
            100,
            VoiceStatusData {
                is_voice_enabled: false,
                is_user_opt_in: true,
                is_user_eligible: false,
                is_banned: true,
                ban_reason: Some(7),
                banned_until_ms: Some(10_000),
                denial_reason: Some(6),
            },
        );

        assert!(matches!(
            state.voice_state,
            VoiceState::TempSuspended { .. }
        ));
        assert!(state.countdown.is_some());
    }
}
