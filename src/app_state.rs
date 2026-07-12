use crate::countdown::AnchoredCountdown;
use crate::messages::{VoiceStatusData, VoiceStatusEnvelope, VoiceStatusErrorKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceState {
    Disconnected,
    Connected,
    Checking,
    VoiceOk,
    TempSuspended,
    SuspendedUnknownDuration,
    Ineligible,
    AuthError,
    NetworkError,
    RateLimited,
    Restored,
}

impl VoiceState {
    pub fn label(&self) -> &'static str {
        match self {
            VoiceState::Disconnected => "Disconnected",
            VoiceState::Connected => "Connected",
            VoiceState::Checking => "Checking voice status",
            VoiceState::VoiceOk => "Voice chat available",
            VoiceState::TempSuspended => "Voice chat suspended",
            VoiceState::SuspendedUnknownDuration => "Suspended, duration unknown",
            VoiceState::Ineligible => "Voice chat unavailable",
            VoiceState::AuthError => "Logged out",
            VoiceState::NetworkError => "Network error",
            VoiceState::RateLimited => "Rate limited",
            VoiceState::Restored => "Voice chat restored",
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

    pub fn mark_restored(&mut self) {
        self.countdown = None;
        self.voice_state = VoiceState::Restored;
    }

    pub fn mark_test_suspended(&mut self, checked_at_ms: i64, banned_until_ms: i64) {
        self.last_checked_at_ms = Some(checked_at_ms);
        self.restored_overlay_shown = false;
        self.countdown = Some(AnchoredCountdown::new(banned_until_ms));
        self.voice_state = VoiceState::TempSuspended;
    }

    pub fn apply_voice_status(&mut self, envelope: VoiceStatusEnvelope) {
        self.browser_connected = true;
        self.last_checked_at_ms = Some(envelope.checked_at);

        if !envelope.ok {
            if self.has_known_suspension() {
                return;
            }
            self.countdown = None;
            let Some(error) = envelope.error else {
                self.voice_state = VoiceState::NetworkError;
                return;
            };

            self.voice_state = match error.kind {
                VoiceStatusErrorKind::AuthError => VoiceState::AuthError,
                VoiceStatusErrorKind::RateLimited => VoiceState::RateLimited,
                VoiceStatusErrorKind::NetworkError | VoiceStatusErrorKind::UnexpectedResponse => {
                    VoiceState::NetworkError
                }
            };
            return;
        }

        let Some(data) = envelope.data else {
            if self.has_known_suspension() {
                return;
            }
            self.countdown = None;
            self.voice_state = VoiceState::NetworkError;
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
                    self.voice_state = VoiceState::TempSuspended;
                }
                None => {
                    self.countdown = None;
                    self.voice_state = VoiceState::SuspendedUnknownDuration;
                }
            }
            return;
        }

        self.countdown = None;
        if data.is_voice_enabled && data.is_user_opt_in && data.is_user_eligible {
            self.voice_state = if matches!(
                self.voice_state,
                VoiceState::TempSuspended
                    | VoiceState::SuspendedUnknownDuration
                    | VoiceState::Restored
            ) {
                VoiceState::Restored
            } else {
                VoiceState::VoiceOk
            };
        } else {
            self.voice_state = VoiceState::Ineligible;
        }
    }

    fn has_known_suspension(&self) -> bool {
        matches!(
            self.voice_state,
            VoiceState::TempSuspended | VoiceState::SuspendedUnknownDuration
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::VoiceStatusError;

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

        assert!(matches!(state.voice_state, VoiceState::TempSuspended));
        assert!(state.countdown.is_some());
    }

    #[test]
    fn auth_error_keeps_browser_connected_and_shows_logged_out() {
        let mut state = AppState::default();
        state.apply_voice_status(VoiceStatusEnvelope {
            request_id: "auth-check".into(),
            checked_at: 100,
            ok: false,
            data: None,
            error: Some(VoiceStatusError {
                kind: VoiceStatusErrorKind::AuthError,
                message: "Please log in to Roblox in this browser.".into(),
                retry_after_ms: None,
            }),
        });

        assert!(state.is_browser_connected());
        assert!(matches!(state.voice_state, VoiceState::AuthError));
        assert_eq!(state.voice_state.label(), "Logged out");
    }

    #[test]
    fn failed_check_does_not_discard_a_known_suspension() {
        let mut state = AppState::default();
        state.mark_test_suspended(100, 10_000);
        state.apply_voice_status(VoiceStatusEnvelope {
            request_id: "failed-check".into(),
            checked_at: 200,
            ok: false,
            data: None,
            error: Some(VoiceStatusError {
                kind: VoiceStatusErrorKind::NetworkError,
                message: "offline".into(),
                retry_after_ms: None,
            }),
        });

        assert!(matches!(state.voice_state, VoiceState::TempSuspended));
        assert!(state.countdown.is_some());
        assert_eq!(state.last_checked_at_ms, Some(200));
    }

    #[test]
    fn local_countdown_expiry_preserves_the_last_real_check_time() {
        let mut state = AppState::default();
        state.mark_test_suspended(100, 10_000);
        state.mark_restored();

        assert!(matches!(state.voice_state, VoiceState::Restored));
        assert_eq!(state.last_checked_at_ms, Some(100));
    }
}
