#![allow(dead_code)]

use crate::messages::VoiceStatusEnvelope;

#[derive(Debug, Clone)]
pub enum IpcEvent {
    VoiceStatus(VoiceStatusEnvelope),
    ExtensionConnected,
    ExtensionDisconnected,
}

pub trait IpcBridge {
    fn publish(&self, event: IpcEvent) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct NoopIpcBridge;

impl IpcBridge for NoopIpcBridge {
    fn publish(&self, _event: IpcEvent) -> anyhow::Result<()> {
        Ok(())
    }
}
