use crate::settings::Settings;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollDecision {
    SkipRobloxNotRunning,
    Wait,
    RequestStatus { request_id: String },
}

#[derive(Debug)]
pub struct PollMonitor {
    settings: Settings,
    last_request_at: Option<Instant>,
    request_in_flight: bool,
    backoff_until: Option<Instant>,
}

impl PollMonitor {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            last_request_at: None,
            request_in_flight: false,
            backoff_until: None,
        }
    }

    pub fn next_decision(&mut self, roblox_running: bool) -> PollDecision {
        if self.settings.only_poll_when_roblox_running && !roblox_running {
            return PollDecision::SkipRobloxNotRunning;
        }

        if self.request_in_flight {
            return PollDecision::Wait;
        }

        if self
            .backoff_until
            .is_some_and(|backoff_until| Instant::now() < backoff_until)
        {
            return PollDecision::Wait;
        }

        let interval = Duration::from_secs(self.settings.poll_interval_seconds);
        if self
            .last_request_at
            .is_some_and(|last_request_at| last_request_at.elapsed() < interval)
        {
            return PollDecision::Wait;
        }

        self.request_in_flight = true;
        self.last_request_at = Some(Instant::now());
        PollDecision::RequestStatus {
            request_id: Uuid::new_v4().to_string(),
        }
    }

    pub fn complete_request(&mut self) {
        self.request_in_flight = false;
    }

    pub fn backoff_for(&mut self, duration: Duration) {
        self.request_in_flight = false;
        self.backoff_until = Some(Instant::now() + duration);
    }
}

