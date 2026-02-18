//! Storage for pairing keys

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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
#[async_trait]
pub trait PairingStorage: Send + Sync {
    /// Load keys for a device
    async fn load(&self, device_id: &str) -> Option<PairingKeys>;

    /// Save keys for a device
    ///
    /// # Errors
    ///
    /// Returns error if storage fails
    async fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError>;

    /// Remove keys for a device
    ///
    /// # Errors
    ///
    /// Returns error if removal fails
    async fn remove(&mut self, device_id: &str) -> Result<(), StorageError>;

    /// List all stored device IDs
    async fn list_devices(&self) -> Vec<String>;
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

#[async_trait]
impl PairingStorage for MemoryStorage {
    async fn load(&self, device_id: &str) -> Option<PairingKeys> {
        self.keys.get(device_id).cloned()
    }

    async fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError> {
        self.keys.insert(device_id.to_string(), keys.clone());
        Ok(())
    }

    async fn remove(&mut self, device_id: &str) -> Result<(), StorageError> {
        self.keys.remove(device_id);
        Ok(())
    }

    async fn list_devices(&self) -> Vec<String> {
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
    /// Returns error if directory cannot be created or file loaded
    pub async fn new(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Load existing keys
        let cache = Self::load_all(&path).await?;

        Ok(Self { path, cache })
    }

    async fn load_all(
        path: &std::path::Path,
    ) -> Result<HashMap<String, PairingKeys>, StorageError> {
        if !tokio::fs::try_exists(path).await? {
            return Ok(HashMap::new());
        }

        let bytes = tokio::fs::read(path).await?;
        if bytes.is_empty() {
            return Ok(HashMap::new());
        }

        let cache = tokio::task::spawn_blocking(move || serde_json::from_slice(&bytes))
            .await
            .map_err(|e| StorageError::Serialization(format!("Deserialization task failed: {e}")))?
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        Ok(cache)
    }

    async fn save_all(&self) -> Result<(), StorageError> {
        let path = self.path.clone();
        let cache = self.cache.clone();

        let bytes = tokio::task::spawn_blocking(move || serde_json::to_vec_pretty(&cache))
            .await
            .map_err(|e| StorageError::Serialization(format!("Serialization task failed: {e}")))?
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        tokio::fs::write(path, bytes).await?;
        Ok(())
    }
}

#[async_trait]
impl PairingStorage for FileStorage {
    async fn load(&self, device_id: &str) -> Option<PairingKeys> {
        self.cache.get(device_id).cloned()
    }

    async fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError> {
        self.cache.insert(device_id.to_string(), keys.clone());
        self.save_all().await
    }

    async fn remove(&mut self, device_id: &str) -> Result<(), StorageError> {
        self.cache.remove(device_id);
        self.save_all().await
    }

    async fn list_devices(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }
}
