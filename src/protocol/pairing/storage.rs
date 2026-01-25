//! Storage for pairing keys

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stored pairing keys for a device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingKeys {
    /// Our identifier (e.g., "airplay2-rs")
    pub identifier: Vec<u8>,
    /// Our Ed25519 secret key (32 bytes)
    pub secret_key: [u8; 32],
    /// Our Ed25519 public key (32 bytes)
    pub public_key: [u8; 32],
    /// Device's Ed25519 public key (32 bytes)
    pub device_public_key: [u8; 32],
}

/// Abstract storage interface for pairing keys
pub trait PairingStorage: Send + Sync {
    /// Load keys for a device
    fn load(&self, device_id: &str) -> Option<PairingKeys>;

    /// Save keys for a device
    ///
    /// # Errors
    ///
    /// Returns error if storage fails
    fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError>;

    /// Remove keys for a device
    ///
    /// # Errors
    ///
    /// Returns error if removal fails
    fn remove(&mut self, device_id: &str) -> Result<(), StorageError>;

    /// List all stored device IDs
    fn list_devices(&self) -> Vec<String>;
}

/// Storage errors
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("storage not available")]
    NotAvailable,
}

/// In-memory pairing storage (non-persistent)
#[derive(Debug, Default)]
pub struct MemoryStorage {
    keys: HashMap<String, PairingKeys>,
}

impl MemoryStorage {
    /// Create a new in-memory storage
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl PairingStorage for MemoryStorage {
    fn load(&self, device_id: &str) -> Option<PairingKeys> {
        self.keys.get(device_id).cloned()
    }

    fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError> {
        self.keys.insert(device_id.to_string(), keys.clone());
        Ok(())
    }

    fn remove(&mut self, device_id: &str) -> Result<(), StorageError> {
        self.keys.remove(device_id);
        Ok(())
    }

    fn list_devices(&self) -> Vec<String> {
        self.keys.keys().cloned().collect()
    }
}

/// File-based pairing storage
pub struct FileStorage {
    #[allow(dead_code)]
    path: std::path::PathBuf,
    cache: HashMap<String, PairingKeys>,
}

impl FileStorage {
    /// Create file storage at the given path
    ///
    /// # Errors
    ///
    /// Returns error if directory cannot be created
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Load existing keys
        let cache = Self::load_all(&path)?;

        Ok(Self { path, cache })
    }

    fn load_all(path: &std::path::Path) -> Result<HashMap<String, PairingKeys>, StorageError> {
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let cache = serde_json::from_reader(reader)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        Ok(cache)
    }

    fn save_all(&self) -> Result<(), StorageError> {
        let file = std::fs::File::create(&self.path)?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.cache)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        Ok(())
    }
}

impl PairingStorage for FileStorage {
    fn load(&self, device_id: &str) -> Option<PairingKeys> {
        self.cache.get(device_id).cloned()
    }

    fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError> {
        self.cache.insert(device_id.to_string(), keys.clone());
        self.save_all()
    }

    fn remove(&mut self, device_id: &str) -> Result<(), StorageError> {
        self.cache.remove(device_id);
        self.save_all()
    }

    fn list_devices(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }
}
