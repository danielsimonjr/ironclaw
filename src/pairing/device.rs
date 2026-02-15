//! Device pairing for multi-device support.
//!
//! Allows multiple devices to register and pair with the agent.
//! Each device goes through a challenge-based approval flow before
//! being granted trusted status.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Platform a device is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(non_camel_case_types)]
pub enum Platform {
    MacOS,
    Linux,
    Windows,
    #[serde(rename = "ios")]
    iOS,
    Android,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::MacOS => write!(f, "macOS"),
            Platform::Linux => write!(f, "Linux"),
            Platform::Windows => write!(f, "Windows"),
            Platform::iOS => write!(f, "iOS"),
            Platform::Android => write!(f, "Android"),
        }
    }
}

/// Information about a registered device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Unique identifier for the device.
    pub device_id: String,
    /// Human-readable device name.
    pub name: String,
    /// Platform the device runs on.
    pub platform: Platform,
    /// When the device was first paired.
    pub paired_at: DateTime<Utc>,
    /// When the device was last seen active.
    pub last_seen: DateTime<Utc>,
    /// Whether this device is trusted (approved).
    pub is_trusted: bool,
}

/// A pairing challenge issued to a device during registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingChallenge {
    /// Six-digit challenge code the user must confirm.
    pub challenge_code: String,
    /// When this challenge expires.
    pub expires_at: DateTime<Utc>,
    /// The device requesting pairing.
    pub device_info: DeviceInfo,
}

/// Default challenge expiry duration in minutes.
const CHALLENGE_EXPIRY_MINUTES: i64 = 10;

/// Error type for device pairing operations.
#[derive(Debug, thiserror::Error)]
pub enum DevicePairingError {
    #[error("Device not found: {device_id}")]
    DeviceNotFound { device_id: String },

    #[error("Challenge not found or expired for device: {device_id}")]
    ChallengeNotFound { device_id: String },

    #[error("Challenge expired for device: {device_id}")]
    ChallengeExpired { device_id: String },

    #[error("Device already registered: {device_id}")]
    AlreadyRegistered { device_id: String },

    #[error("Invalid challenge code for device: {device_id}")]
    InvalidChallengeCode { device_id: String },
}

/// Manages device registration, approval, and trust.
///
/// Uses in-memory storage backed by `Arc<RwLock<HashMap>>` for
/// concurrent access from multiple async tasks.
#[derive(Debug, Clone)]
pub struct DevicePairingManager {
    /// Registered devices indexed by device_id.
    devices: Arc<RwLock<HashMap<String, DeviceInfo>>>,
    /// Pending pairing challenges indexed by device_id.
    challenges: Arc<RwLock<HashMap<String, PairingChallenge>>>,
}

impl DevicePairingManager {
    /// Create a new device pairing manager with empty state.
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            challenges: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new device and issue a pairing challenge.
    ///
    /// Returns a `PairingChallenge` containing the 6-digit code the user
    /// must provide to approve the device.
    pub async fn register_device(
        &self,
        device_id: String,
        name: String,
        platform: Platform,
    ) -> Result<PairingChallenge, DevicePairingError> {
        let devices = self.devices.read().await;
        if devices.contains_key(&device_id) {
            return Err(DevicePairingError::AlreadyRegistered {
                device_id: device_id.clone(),
            });
        }
        drop(devices);

        let now = Utc::now();
        let device_info = DeviceInfo {
            device_id: device_id.clone(),
            name,
            platform,
            paired_at: now,
            last_seen: now,
            is_trusted: false,
        };

        let challenge_code = generate_challenge_code();
        let expires_at = now + Duration::minutes(CHALLENGE_EXPIRY_MINUTES);

        let challenge = PairingChallenge {
            challenge_code,
            expires_at,
            device_info: device_info.clone(),
        };

        // Store the device as untrusted until approved.
        self.devices
            .write()
            .await
            .insert(device_id.clone(), device_info);

        // Store the challenge for later verification.
        self.challenges
            .write()
            .await
            .insert(device_id, challenge.clone());

        Ok(challenge)
    }

    /// Approve a device by verifying its challenge code.
    ///
    /// Marks the device as trusted if the code matches and hasn't expired.
    pub async fn approve_device(
        &self,
        device_id: &str,
        challenge_code: &str,
    ) -> Result<DeviceInfo, DevicePairingError> {
        let mut challenges = self.challenges.write().await;
        let challenge =
            challenges
                .remove(device_id)
                .ok_or_else(|| DevicePairingError::ChallengeNotFound {
                    device_id: device_id.to_string(),
                })?;

        if Utc::now() > challenge.expires_at {
            return Err(DevicePairingError::ChallengeExpired {
                device_id: device_id.to_string(),
            });
        }

        if challenge.challenge_code != challenge_code {
            // Re-insert the challenge so the user can retry.
            challenges.insert(device_id.to_string(), challenge);
            return Err(DevicePairingError::InvalidChallengeCode {
                device_id: device_id.to_string(),
            });
        }
        drop(challenges);

        let mut devices = self.devices.write().await;
        let device =
            devices
                .get_mut(device_id)
                .ok_or_else(|| DevicePairingError::DeviceNotFound {
                    device_id: device_id.to_string(),
                })?;

        device.is_trusted = true;
        device.last_seen = Utc::now();
        Ok(device.clone())
    }

    /// Reject a pending device pairing, removing both the challenge and device record.
    pub async fn reject_device(&self, device_id: &str) -> Result<DeviceInfo, DevicePairingError> {
        self.challenges.write().await.remove(device_id);

        let device = self
            .devices
            .write()
            .await
            .remove(device_id)
            .ok_or_else(|| DevicePairingError::DeviceNotFound {
                device_id: device_id.to_string(),
            })?;

        Ok(device)
    }

    /// List all registered devices (both trusted and untrusted).
    pub async fn list_devices(&self) -> Vec<DeviceInfo> {
        let devices = self.devices.read().await;
        let mut list: Vec<DeviceInfo> = devices.values().cloned().collect();
        list.sort_by(|a, b| a.paired_at.cmp(&b.paired_at));
        list
    }

    /// Remove a registered device by its ID.
    pub async fn remove_device(&self, device_id: &str) -> Result<DeviceInfo, DevicePairingError> {
        // Also clean up any pending challenge.
        self.challenges.write().await.remove(device_id);

        self.devices.write().await.remove(device_id).ok_or_else(|| {
            DevicePairingError::DeviceNotFound {
                device_id: device_id.to_string(),
            }
        })
    }

    /// Get information about a specific device.
    pub async fn get_device(&self, device_id: &str) -> Result<DeviceInfo, DevicePairingError> {
        self.devices
            .read()
            .await
            .get(device_id)
            .cloned()
            .ok_or_else(|| DevicePairingError::DeviceNotFound {
                device_id: device_id.to_string(),
            })
    }

    /// Check whether a device is trusted (approved).
    ///
    /// Returns `false` if the device is not found or not yet approved.
    pub async fn is_device_trusted(&self, device_id: &str) -> bool {
        self.devices
            .read()
            .await
            .get(device_id)
            .is_some_and(|d| d.is_trusted)
    }
}

impl Default for DevicePairingManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a random 6-digit challenge code.
fn generate_challenge_code() -> String {
    let mut rng = rand::thread_rng();
    let code: u32 = rng.gen_range(100_000..1_000_000);
    code.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_device() {
        let manager = DevicePairingManager::new();
        let challenge = manager
            .register_device(
                "dev-1".to_string(),
                "My Laptop".to_string(),
                Platform::MacOS,
            )
            .await
            .unwrap();

        assert_eq!(challenge.challenge_code.len(), 6);
        assert_eq!(challenge.device_info.device_id, "dev-1");
        assert_eq!(challenge.device_info.name, "My Laptop");
        assert_eq!(challenge.device_info.platform, Platform::MacOS);
        assert!(!challenge.device_info.is_trusted);
    }

    #[tokio::test]
    async fn test_register_duplicate_device_rejected() {
        let manager = DevicePairingManager::new();
        manager
            .register_device("dev-1".to_string(), "Laptop".to_string(), Platform::Linux)
            .await
            .unwrap();

        let err = manager
            .register_device("dev-1".to_string(), "Laptop 2".to_string(), Platform::Linux)
            .await
            .unwrap_err();

        assert!(matches!(err, DevicePairingError::AlreadyRegistered { .. }));
    }

    #[tokio::test]
    async fn test_approve_device_with_correct_code() {
        let manager = DevicePairingManager::new();
        let challenge = manager
            .register_device("dev-2".to_string(), "Phone".to_string(), Platform::Android)
            .await
            .unwrap();

        let device = manager
            .approve_device("dev-2", &challenge.challenge_code)
            .await
            .unwrap();

        assert!(device.is_trusted);
        assert_eq!(device.device_id, "dev-2");
        assert!(manager.is_device_trusted("dev-2").await);
    }

    #[tokio::test]
    async fn test_approve_device_with_wrong_code() {
        let manager = DevicePairingManager::new();
        manager
            .register_device("dev-3".to_string(), "Tablet".to_string(), Platform::iOS)
            .await
            .unwrap();

        let err = manager.approve_device("dev-3", "000000").await.unwrap_err();

        assert!(matches!(
            err,
            DevicePairingError::InvalidChallengeCode { .. }
        ));
        assert!(!manager.is_device_trusted("dev-3").await);
    }

    #[tokio::test]
    async fn test_reject_device() {
        let manager = DevicePairingManager::new();
        manager
            .register_device(
                "dev-4".to_string(),
                "Unknown Device".to_string(),
                Platform::Windows,
            )
            .await
            .unwrap();

        let rejected = manager.reject_device("dev-4").await.unwrap();
        assert_eq!(rejected.device_id, "dev-4");

        let err = manager.get_device("dev-4").await.unwrap_err();
        assert!(matches!(err, DevicePairingError::DeviceNotFound { .. }));
    }

    #[tokio::test]
    async fn test_list_devices() {
        let manager = DevicePairingManager::new();
        manager
            .register_device("dev-a".to_string(), "Device A".to_string(), Platform::MacOS)
            .await
            .unwrap();
        manager
            .register_device("dev-b".to_string(), "Device B".to_string(), Platform::Linux)
            .await
            .unwrap();

        let devices = manager.list_devices().await;
        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_device() {
        let manager = DevicePairingManager::new();
        manager
            .register_device("dev-5".to_string(), "Old Phone".to_string(), Platform::iOS)
            .await
            .unwrap();

        let removed = manager.remove_device("dev-5").await.unwrap();
        assert_eq!(removed.device_id, "dev-5");
        assert!(manager.list_devices().await.is_empty());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_device() {
        let manager = DevicePairingManager::new();
        let err = manager.remove_device("nonexistent").await.unwrap_err();
        assert!(matches!(err, DevicePairingError::DeviceNotFound { .. }));
    }

    #[tokio::test]
    async fn test_get_device() {
        let manager = DevicePairingManager::new();
        manager
            .register_device(
                "dev-6".to_string(),
                "Desktop".to_string(),
                Platform::Windows,
            )
            .await
            .unwrap();

        let device = manager.get_device("dev-6").await.unwrap();
        assert_eq!(device.name, "Desktop");
        assert_eq!(device.platform, Platform::Windows);
    }

    #[tokio::test]
    async fn test_is_device_trusted_false_for_unknown() {
        let manager = DevicePairingManager::new();
        assert!(!manager.is_device_trusted("unknown-dev").await);
    }

    #[tokio::test]
    async fn test_is_device_trusted_false_before_approval() {
        let manager = DevicePairingManager::new();
        manager
            .register_device(
                "dev-7".to_string(),
                "Pending".to_string(),
                Platform::Android,
            )
            .await
            .unwrap();

        assert!(!manager.is_device_trusted("dev-7").await);
    }

    #[tokio::test]
    async fn test_challenge_code_is_six_digits() {
        for _ in 0..100 {
            let code = generate_challenge_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
            let num: u32 = code.parse().unwrap();
            assert!((100_000..1_000_000).contains(&num));
        }
    }

    #[tokio::test]
    async fn test_approve_clears_challenge() {
        let manager = DevicePairingManager::new();
        let challenge = manager
            .register_device(
                "dev-8".to_string(),
                "Clearable".to_string(),
                Platform::Linux,
            )
            .await
            .unwrap();

        manager
            .approve_device("dev-8", &challenge.challenge_code)
            .await
            .unwrap();

        // Trying to approve again should fail because the challenge was consumed.
        let err = manager
            .approve_device("dev-8", &challenge.challenge_code)
            .await
            .unwrap_err();
        assert!(matches!(err, DevicePairingError::ChallengeNotFound { .. }));
    }

    #[tokio::test]
    async fn test_platform_display() {
        assert_eq!(Platform::MacOS.to_string(), "macOS");
        assert_eq!(Platform::Linux.to_string(), "Linux");
        assert_eq!(Platform::Windows.to_string(), "Windows");
        assert_eq!(Platform::iOS.to_string(), "iOS");
        assert_eq!(Platform::Android.to_string(), "Android");
    }
}
