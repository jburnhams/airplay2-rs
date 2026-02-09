//! AirPlay 2 Service Advertisement
//!
//! Handles mDNS advertisement for the AirPlay 2 receiver, making it
//! discoverable by iOS/macOS devices on the local network.

use crate::receiver::ap2::config::Ap2Config;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// TXT record keys for AirPlay 2 service advertisement
pub mod txt_keys {
    /// Access control level (0=any, 1=same network)
    pub const ACL: &str = "acl";
    /// Device ID (MAC address format)
    pub const DEVICE_ID: &str = "deviceid";
    /// Feature flags (64-bit hex)
    pub const FEATURES: &str = "features";
    /// Firmware source version
    pub const FIRMWARE_VERSION: &str = "fv";
    /// Group UUID (for multi-room)
    pub const GROUP_UUID: &str = "gid";
    /// Model identifier
    pub const MODEL: &str = "model";
    /// Protocol version
    pub const PROTOCOL_VERSION: &str = "protovers";
    /// Public key (Ed25519, base64)
    pub const PUBLIC_KEY: &str = "pk";
    /// Pairing identity (UUID)
    pub const PAIRING_IDENTITY: &str = "pi";
    /// Required sender features (hex)
    pub const REQUIRED_SENDER: &str = "rsf";
    /// Source version
    pub const SOURCE_VERSION: &str = "srcvers";
    /// Status flags
    pub const STATUS_FLAGS: &str = "flags";
    /// Serial number
    pub const SERIAL_NUMBER: &str = "serialNumber";
    /// Manufacturer
    pub const MANUFACTURER: &str = "manufacturer";
    /// OS build version
    pub const OS_BUILD: &str = "osvers";
    /// Volume control type
    pub const VOLUME_CONTROL: &str = "vv";
}

/// Builder for AirPlay 2 TXT records
#[derive(Debug, Clone)]
pub struct Ap2TxtRecord {
    entries: HashMap<String, String>,
}

impl Ap2TxtRecord {
    /// Create TXT record from receiver configuration
    pub fn from_config(config: &Ap2Config, public_key: &[u8; 32]) -> Self {
        let mut entries = HashMap::new();

        // Device identification
        entries.insert(
            txt_keys::DEVICE_ID.to_string(),
            config.device_id.clone(),
        );
        entries.insert(
            txt_keys::MODEL.to_string(),
            config.model.clone(),
        );
        entries.insert(
            txt_keys::MANUFACTURER.to_string(),
            config.manufacturer.clone(),
        );

        if let Some(ref serial) = config.serial_number {
            entries.insert(
                txt_keys::SERIAL_NUMBER.to_string(),
                serial.clone(),
            );
        }

        // Version information
        entries.insert(
            txt_keys::FIRMWARE_VERSION.to_string(),
            config.firmware_version.clone(),
        );
        entries.insert(
            txt_keys::SOURCE_VERSION.to_string(),
            "366.0".to_string(),  // AirPlay 2 protocol version
        );
        entries.insert(
            txt_keys::PROTOCOL_VERSION.to_string(),
            "1.1".to_string(),
        );

        // Feature flags (split into two 32-bit parts for compatibility)
        let features = config.feature_flags();
        let features_str = format!("0x{:X},0x{:X}", features & 0xFFFFFFFF, features >> 32);
        entries.insert(txt_keys::FEATURES.to_string(), features_str);

        // Status flags
        entries.insert(
            txt_keys::STATUS_FLAGS.to_string(),
            format!("0x{:X}", config.status_flags()),
        );

        // Public key for pairing (Ed25519, base64 encoded)
        use base64::Engine;
        let pk_b64 = base64::engine::general_purpose::STANDARD.encode(public_key);
        entries.insert(txt_keys::PUBLIC_KEY.to_string(), pk_b64);

        // Pairing identity (UUID derived from device ID)
        let pi = Self::derive_pairing_identity(&config.device_id);
        entries.insert(txt_keys::PAIRING_IDENTITY.to_string(), pi);

        // Access control (0 = open, 1 = requires same network)
        entries.insert(txt_keys::ACL.to_string(), "0".to_string());

        // Volume control type (2 = hardware volume control available)
        entries.insert(txt_keys::VOLUME_CONTROL.to_string(), "2".to_string());

        Self { entries }
    }

    /// Derive pairing identity UUID from device ID
    fn derive_pairing_identity(device_id: &str) -> String {
        use sha2::{Sha256, Digest};

        // Hash device ID to create deterministic UUID
        let mut hasher = Sha256::new();
        hasher.update(device_id.as_bytes());
        hasher.update(b"AirPlay2-PI");
        let hash = hasher.finalize();

        // Format as UUID (version 4 format, but deterministic)
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            hash[0], hash[1], hash[2], hash[3],
            hash[4], hash[5],
            (hash[6] & 0x0f) | 0x40, hash[7],  // Version 4
            (hash[8] & 0x3f) | 0x80, hash[9],  // Variant 1
            hash[10], hash[11], hash[12], hash[13], hash[14], hash[15]
        )
    }

    /// Get all entries as key-value pairs
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Get a specific entry
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    /// Update an entry
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.insert(key.into(), value.into());
    }

    /// Convert to mdns-sd compatible format
    pub fn to_txt_properties(&self) -> Vec<(String, String)> {
        self.entries
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// Service type for AirPlay 2
pub const AIRPLAY2_SERVICE_TYPE: &str = "_airplay._tcp.local.";

/// AirPlay 2 service advertiser
///
/// Manages mDNS advertisement of the receiver on the local network.
/// Uses the same mdns-sd crate as the discovery module.
pub struct Ap2ServiceAdvertiser {
    config: Ap2Config,
    daemon: ServiceDaemon,
    service_info: Arc<RwLock<Option<ServiceInfo>>>,
    keypair: Ed25519Keypair,
}

/// Ed25519 keypair for pairing
struct Ed25519Keypair {
    public: [u8; 32],
    #[allow(dead_code)]
    secret: [u8; 32],
}

impl Ap2ServiceAdvertiser {
    /// Create a new advertiser with the given configuration
    pub fn new(config: Ap2Config) -> Result<Self, AdvertisementError> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| AdvertisementError::MdnsInit(e.to_string()))?;

        // Generate or load keypair for this device
        let keypair = Self::generate_keypair(&config.device_id);

        Ok(Self {
            config,
            daemon,
            service_info: Arc::new(RwLock::new(None)),
            keypair,
        })
    }

    /// Generate deterministic Ed25519 keypair from device ID
    fn generate_keypair(device_id: &str) -> Ed25519Keypair {
        use sha2::{Sha256, Digest};

        // Derive seed from device ID (deterministic)
        let mut hasher = Sha256::new();
        hasher.update(device_id.as_bytes());
        hasher.update(b"AirPlay2-Ed25519-Seed");
        let seed: [u8; 32] = hasher.finalize().into();

        // Generate Ed25519 keypair from seed
        use ed25519_dalek::SigningKey;
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();

        Ed25519Keypair {
            public: verifying_key.to_bytes(),
            secret: seed,
        }
    }

    /// Start advertising the service
    pub async fn start(&self) -> Result<(), AdvertisementError> {
        let txt = Ap2TxtRecord::from_config(&self.config, &self.keypair.public);

        // Get local hostname
        let hostname = hostname::get()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|_| "airplay-receiver".to_string());

        let service_info = ServiceInfo::new(
            AIRPLAY2_SERVICE_TYPE,
            &self.config.name,
            &format!("{}.local.", hostname),
            "",  // Let mdns-sd determine IP
            self.config.server_port,
            txt.to_txt_properties()
                .into_iter()
                .collect::<HashMap<String, String>>(),
        )
        .map_err(|e| AdvertisementError::ServiceCreate(e.to_string()))?;

        // Register with daemon
        self.daemon
            .register(service_info.clone())
            .map_err(|e| AdvertisementError::Registration(e.to_string()))?;

        // Store for later updates/unregistration
        *self.service_info.write().await = Some(service_info);

        tracing::info!(
            "AirPlay 2 service advertised: {} on port {}",
            self.config.name,
            self.config.server_port
        );

        Ok(())
    }

    /// Stop advertising the service
    pub async fn stop(&self) -> Result<(), AdvertisementError> {
        if let Some(service_info) = self.service_info.write().await.take() {
            self.daemon
                .unregister(service_info.get_fullname())
                .map_err(|e| AdvertisementError::Unregistration(e.to_string()))?;

            tracing::info!("AirPlay 2 service unregistered: {}", self.config.name);
        }

        Ok(())
    }

    /// Update the advertised service name
    pub async fn update_name(&mut self, new_name: String) -> Result<(), AdvertisementError> {
        // Stop current advertisement
        self.stop().await?;

        // Update config
        self.config.name = new_name;

        // Re-advertise
        self.start().await
    }

    /// Update feature flags (e.g., when multi-room is enabled/disabled)
    pub async fn update_features(&self) -> Result<(), AdvertisementError> {
        // Re-register with updated TXT record
        self.stop().await?;
        self.start().await
    }

    /// Get the public key for pairing
    pub fn public_key(&self) -> &[u8; 32] {
        &self.keypair.public
    }

    /// Get the current configuration
    pub fn config(&self) -> &Ap2Config {
        &self.config
    }
}

/// Errors from service advertisement
#[derive(Debug, thiserror::Error)]
pub enum AdvertisementError {
    /// Failed to initialize mDNS daemon
    #[error("Failed to initialize mDNS daemon: {0}")]
    MdnsInit(String),

    /// Failed to create service info
    #[error("Failed to create service info: {0}")]
    ServiceCreate(String),

    /// Failed to register service
    #[error("Failed to register service: {0}")]
    Registration(String),

    /// Failed to unregister service
    #[error("Failed to unregister service: {0}")]
    Unregistration(String),
}
