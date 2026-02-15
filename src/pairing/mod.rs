//! DM pairing for channels.
//!
//! Gates DMs from unknown senders. Only approved senders can message the agent.
//! Unknown senders receive a pairing code and must be approved via `ironclaw pairing approve`.
//!
//! OpenClaw reference: src/pairing/pairing-store.ts

pub mod device;
mod store;

pub use device::{
    DeviceInfo, DevicePairingError, DevicePairingManager, PairingChallenge, Platform,
};
pub use store::{PairingRequest, PairingStore, PairingStoreError};
