//! DACP service advertisement

use std::collections::HashMap;

use super::{DACP_DEFAULT_PORT, txt_keys};

/// DACP service configuration
#[derive(Debug, Clone)]
pub struct DacpServiceConfig {
    /// DACP ID (64-bit identifier)
    pub dacp_id: String,
    /// Active remote token
    pub active_remote: String,
    /// Service port
    pub port: u16,
    /// Database ID (typically same as DACP ID)
    pub db_id: String,
}

impl DacpServiceConfig {
    /// Create new configuration with random identifiers
    #[must_use]
    pub fn new() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let dacp_id = format!("{:016X}", rng.r#gen::<u64>());
        let active_remote = rng.r#gen::<u32>().to_string();

        Self {
            dacp_id: dacp_id.clone(),
            active_remote,
            port: DACP_DEFAULT_PORT,
            db_id: dacp_id,
        }
    }

    /// Get service instance name (`iTunes_Ctrl`_{`DACP_ID`})
    #[must_use]
    pub fn instance_name(&self) -> String {
        format!("iTunes_Ctrl_{}", self.dacp_id)
    }

    /// Get TXT records for service advertisement
    #[must_use]
    pub fn txt_records(&self) -> HashMap<String, String> {
        let mut records = HashMap::new();
        records.insert(txt_keys::TXTVERS.to_string(), "1".to_string());
        records.insert(txt_keys::VER.to_string(), "131073".to_string());
        records.insert(txt_keys::DBID.to_string(), self.db_id.clone());
        records.insert(txt_keys::OSSI.to_string(), "0x1F5".to_string());
        records
    }
}

impl Default for DacpServiceConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// DACP service for mDNS registration
pub struct DacpService {
    /// Configuration
    config: DacpServiceConfig,
    /// Whether service is registered
    registered: bool,
}

impl DacpService {
    /// Create new DACP service
    #[must_use]
    pub fn new(config: DacpServiceConfig) -> Self {
        Self {
            config,
            registered: false,
        }
    }

    /// Get configuration
    #[must_use]
    pub fn config(&self) -> &DacpServiceConfig {
        &self.config
    }

    /// Get DACP-ID header value
    #[must_use]
    pub fn dacp_id(&self) -> &str {
        &self.config.dacp_id
    }

    /// Get Active-Remote header value
    #[must_use]
    pub fn active_remote(&self) -> &str {
        &self.config.active_remote
    }

    /// Register service with mDNS
    ///
    /// # Errors
    ///
    /// Returns error if registration fails
    #[allow(clippy::unused_async)]
    pub async fn register(&mut self) -> Result<(), DacpError> {
        // Use mdns-sd to register service
        // Implementation depends on mDNS library

        self.registered = true;
        Ok(())
    }

    /// Unregister service
    ///
    /// # Errors
    ///
    /// Returns error if unregistration fails
    #[allow(clippy::unused_async)]
    pub async fn unregister(&mut self) -> Result<(), DacpError> {
        self.registered = false;
        Ok(())
    }

    /// Check if service is registered
    #[must_use]
    pub fn is_registered(&self) -> bool {
        self.registered
    }
}

/// DACP errors
#[derive(Debug, thiserror::Error)]
pub enum DacpError {
    #[error("service registration failed: {0}")]
    RegistrationFailed(String),
    #[error("service not registered")]
    NotRegistered,
    #[error("invalid command")]
    InvalidCommand,
    #[error("authentication failed")]
    AuthenticationFailed,
}
