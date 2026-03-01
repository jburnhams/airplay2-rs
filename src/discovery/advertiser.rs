//! RAOP service advertisement for AirPlay 1 receiver

use std::collections::HashMap;
use std::sync::Arc;

use mdns_sd::{Error as MdnsError, ServiceDaemon, ServiceInfo};
use tokio::sync::{Mutex, RwLock, mpsc};

/// Errors from service advertisement
#[derive(Debug, thiserror::Error)]
pub enum AdvertiserError {
    /// Failed to retrieve MAC address
    #[error("Failed to retrieve MAC address: {0}")]
    MacRetrievalFailed(String),

    /// mDNS error
    #[error("mDNS error: {0}")]
    Mdns(#[from] MdnsError),

    /// Service not registered
    #[error("Service not registered")]
    NotRegistered,

    /// Service already registered
    #[error("Service already registered")]
    AlreadyRegistered,
}

/// Retrieve a MAC address for service identification
///
/// The RAOP service name format is `MAC@FriendlyName` where MAC is
/// a 12-character hex string (e.g., "5855CA1AE288").
///
/// Strategy:
/// 1. Try to get the actual hardware MAC of the primary interface
/// 2. Fall back to generating a stable pseudo-MAC from machine ID
/// 3. Last resort: random MAC (not recommended, changes identity)
///
/// # Errors
///
/// Returns error if platform-specific retrieval fails and fallback also fails.
pub fn get_device_mac() -> Result<[u8; 6], AdvertiserError> {
    // Try platform-specific MAC retrieval
    #[cfg(target_os = "macos")]
    {
        get_mac_macos().or_else(|_| Ok(generate_stable_mac()))
    }

    #[cfg(target_os = "linux")]
    {
        get_mac_linux().or_else(|_| Ok(generate_stable_mac()))
    }

    #[cfg(target_os = "windows")]
    {
        get_mac_windows().or_else(|_| Ok(generate_stable_mac()))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        // Generate stable pseudo-MAC
        Ok(generate_stable_mac())
    }
}

#[cfg(target_os = "macos")]
fn get_mac_macos() -> Result<[u8; 6], AdvertiserError> {
    // Use IOKit or system_profiler to get en0 MAC
    // Fallback: parse output of `ifconfig en0`
    Err(AdvertiserError::MacRetrievalFailed(
        "Not implemented".into(),
    ))
}

#[cfg(target_os = "linux")]
fn get_mac_linux() -> Result<[u8; 6], AdvertiserError> {
    // Read from /sys/class/net/<interface>/address
    // Prefer non-loopback, non-virtual interfaces
    use std::fs;

    let net_dir = "/sys/class/net";
    if !std::path::Path::new(net_dir).exists() {
        return Err(AdvertiserError::MacRetrievalFailed(
            "No /sys/class/net found".into(),
        ));
    }

    for entry in
        fs::read_dir(net_dir).map_err(|e| AdvertiserError::MacRetrievalFailed(e.to_string()))?
    {
        let entry = entry.map_err(|e| AdvertiserError::MacRetrievalFailed(e.to_string()))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip loopback and virtual interfaces
        if name_str == "lo" || name_str.starts_with("veth") || name_str.starts_with("docker") {
            continue;
        }

        let addr_path = entry.path().join("address");
        if let Ok(mac_str) = fs::read_to_string(&addr_path) {
            let mac_str = mac_str.trim();
            if mac_str != "00:00:00:00:00:00" {
                return parse_mac_string(mac_str);
            }
        }
    }

    Err(AdvertiserError::MacRetrievalFailed(
        "No suitable interface found".into(),
    ))
}

#[cfg(target_os = "windows")]
fn get_mac_windows() -> Result<[u8; 6], AdvertiserError> {
    Err(AdvertiserError::MacRetrievalFailed(
        "Not implemented".into(),
    ))
}

#[allow(dead_code)]
pub(crate) fn parse_mac_string(mac: &str) -> Result<[u8; 6], AdvertiserError> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 {
        return Err(AdvertiserError::MacRetrievalFailed(format!(
            "Invalid MAC format: {mac}"
        )));
    }

    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .map_err(|_| AdvertiserError::MacRetrievalFailed(format!("Invalid hex: {part}")))?;
    }

    Ok(bytes)
}

// Intentionally extracting bytes from hash
#[allow(
    clippy::cast_possible_truncation,
    reason = "Hash extraction safely truncates to expected mac byte sizes"
)]
pub(crate) fn generate_stable_mac() -> [u8; 6] {
    // Generate from machine-id or hostname hash for stability across restarts
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let seed = std::fs::read_to_string("/etc/machine-id").unwrap_or_else(|_| {
        // Fallback to hostname
        hostname::get().map_or_else(
            |_| "airplay-receiver".to_string(),
            |h| h.to_string_lossy().into_owned(),
        )
    });

    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let hash = hasher.finish();

    // Use hash bytes as MAC, set locally-administered bit
    let mut mac = [0u8; 6];
    mac[0] = ((hash >> 40) as u8) | 0x02; // Set locally-administered bit
    mac[1] = (hash >> 32) as u8;
    mac[2] = (hash >> 24) as u8;
    mac[3] = (hash >> 16) as u8;
    mac[4] = (hash >> 8) as u8;
    mac[5] = hash as u8;

    mac
}

/// Format MAC address for RAOP service name (uppercase, no colons)
#[must_use]
pub fn format_mac_for_service(mac: &[u8; 6]) -> String {
    format!(
        "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

/// RAOP receiver capabilities for TXT record
#[derive(Debug, Clone)]
pub struct RaopCapabilities {
    /// Supported audio codecs: 0=PCM, 1=ALAC, 2=AAC-LC, 3=AAC-ELD
    pub codecs: Vec<u8>,
    /// Supported encryption types: 0=none, 1=RSA+AES
    pub encryption_types: Vec<u8>,
    /// Supported metadata types: 0=text, 1=artwork, 2=progress
    pub metadata_types: Vec<u8>,
    /// Number of audio channels (typically 2 for stereo)
    pub channels: u8,
    /// Sample rate in Hz (typically 44100)
    pub sample_rate: u32,
    /// Sample size in bits (typically 16)
    pub sample_size: u8,
    /// Password required
    pub password_required: bool,
    /// Device model name
    pub model: String,
    /// Protocol version
    pub protocol_version: u8,
    /// Software version string
    pub software_version: String,
}

impl Default for RaopCapabilities {
    fn default() -> Self {
        Self {
            codecs: vec![0, 1, 2],         // PCM, ALAC, AAC-LC
            encryption_types: vec![0, 1],  // None, RSA+AES
            metadata_types: vec![0, 1, 2], // All metadata types
            channels: 2,
            sample_rate: 44_100,
            sample_size: 16,
            password_required: false,
            model: "AirPlayRust".to_string(),
            protocol_version: 1,
            software_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Status flags for the receiver
#[derive(Debug, Clone, Copy, Default)]
// Fields map directly to RAOP status bitmask
#[allow(
    clippy::struct_excessive_bools,
    reason = "Struct accurately models domain logic bools representing RAOP status bitmask"
)]
pub struct ReceiverStatusFlags {
    /// Problem detected (e.g., audio device error)
    pub problem: bool,
    /// Receiver is PIN-protected
    pub pin_required: bool,
    /// Receiver is busy (streaming in progress)
    pub busy: bool,
    /// Supports legacy pairing
    pub supports_legacy_pairing: bool,
}

impl ReceiverStatusFlags {
    /// Convert to the `sf` TXT record value
    #[must_use]
    pub fn to_flags(&self) -> u32 {
        let mut flags = 0u32;

        // Bit positions based on RAOP specification
        if self.problem {
            flags |= 0x01;
        }
        if self.pin_required {
            flags |= 0x02;
        }
        if self.busy {
            flags |= 0x04;
        }
        if self.supports_legacy_pairing {
            flags |= 0x08;
        }

        flags
    }
}

/// Build TXT record for RAOP service advertisement
pub struct TxtRecordBuilder {
    records: HashMap<String, String>,
}

impl TxtRecordBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    /// Build TXT record from capabilities and status
    #[must_use]
    pub fn from_capabilities(caps: &RaopCapabilities, status: &ReceiverStatusFlags) -> Self {
        let mut builder = Self::new();

        // Required fields
        builder.add("txtvers", "1");

        // Audio format
        builder.add("ch", &caps.channels.to_string());
        builder.add("sr", &caps.sample_rate.to_string());
        builder.add("ss", &caps.sample_size.to_string());

        // Codecs (comma-separated)
        builder.add("cn", &Self::format_list(&caps.codecs));

        // Encryption types
        builder.add("et", &Self::format_list(&caps.encryption_types));

        // Metadata types
        builder.add("md", &Self::format_list(&caps.metadata_types));

        // Transport (UDP only for now)
        builder.add("tp", "UDP");

        // Password
        builder.add(
            "pw",
            if caps.password_required {
                "true"
            } else {
                "false"
            },
        );

        // Device info
        builder.add("am", &caps.model);
        builder.add("vn", &caps.protocol_version.to_string());
        builder.add("vs", &caps.software_version);

        // Status flags
        builder.add("sf", &format!("0x{:x}", status.to_flags()));

        // Features (standard RAOP receiver features)
        // This is a bitmask of supported features
        builder.add("ft", "0x4A7FDFD5");

        builder
    }

    /// Add a key-value pair
    pub fn add(&mut self, key: &str, value: &str) -> &mut Self {
        self.records.insert(key.to_string(), value.to_string());
        self
    }

    /// Build into a vector of "key=value" strings
    #[must_use]
    pub fn build(&self) -> Vec<String> {
        self.records
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect()
    }

    /// Build into `HashMap` for mdns-sd
    #[must_use]
    pub fn build_map(&self) -> HashMap<String, String> {
        self.records.clone()
    }

    fn format_list(items: &[u8]) -> String {
        items
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Default for TxtRecordBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for RAOP service advertisement
#[derive(Debug, Clone)]
pub struct AdvertiserConfig {
    /// Friendly name shown to users (e.g., "Living Room Speaker")
    pub name: String,
    /// RTSP port to advertise
    pub port: u16,
    /// Receiver capabilities
    pub capabilities: RaopCapabilities,
    /// Optional: override MAC address
    pub mac_override: Option<[u8; 6]>,
}

impl Default for AdvertiserConfig {
    fn default() -> Self {
        Self {
            name: "AirPlay Receiver".to_string(),
            port: 5000,
            capabilities: RaopCapabilities::default(),
            mac_override: None,
        }
    }
}

/// RAOP service advertiser
///
/// Handles mDNS advertisement lifecycle including registration,
/// status updates, and graceful unregistration.
pub struct RaopAdvertiser {
    config: AdvertiserConfig,
    daemon: ServiceDaemon,
    service_fullname: Option<String>,
    status: Arc<RwLock<ReceiverStatusFlags>>,
    mac: [u8; 6],
}

impl RaopAdvertiser {
    /// Create a new advertiser
    ///
    /// # Errors
    ///
    /// Returns error if mDNS daemon cannot be initialized or MAC address cannot be retrieved.
    pub fn new(config: AdvertiserConfig) -> Result<Self, AdvertiserError> {
        let daemon = ServiceDaemon::new()?;

        let mac = config.mac_override.ok_or_else(|| {
            AdvertiserError::MacRetrievalFailed(
                "MAC address must be provided in config".to_string(),
            )
        })?;

        Ok(Self {
            config,
            daemon,
            service_fullname: None,
            status: Arc::new(RwLock::new(ReceiverStatusFlags::default())),
            mac,
        })
    }

    /// Get the service name that will be advertised
    #[must_use]
    pub fn service_name(&self) -> String {
        format!("{}@{}", format_mac_for_service(&self.mac), self.config.name)
    }

    /// Register the service on the network
    ///
    /// # Errors
    ///
    /// Returns error if service is already registered or mDNS registration fails.
    pub fn register(&mut self) -> Result<(), AdvertiserError> {
        if self.service_fullname.is_some() {
            return Err(AdvertiserError::AlreadyRegistered);
        }

        let service_type = "_raop._tcp.local.";
        let service_name = self.service_name();

        // Build TXT record
        let status = *self.status.blocking_read();
        let txt = TxtRecordBuilder::from_capabilities(&self.config.capabilities, &status);

        // Create service info
        // Note: mdns-sd ServiceInfo requires careful construction
        let hostname = format!(
            "{}.local.",
            self.config.name.replace(' ', "-").to_lowercase()
        );
        let service_info = ServiceInfo::new(
            service_type,
            &service_name,
            &hostname,
            "", // IP addresses (auto-detect)
            self.config.port,
            txt.build_map(),
        )?;

        // Register with daemon
        self.daemon.register(service_info.clone())?;

        self.service_fullname = Some(service_info.get_fullname().to_string());

        tracing::info!(
            name = %service_name,
            port = %self.config.port,
            "RAOP service registered"
        );

        Ok(())
    }

    /// Unregister the service from the network
    ///
    /// # Errors
    ///
    /// Returns error if service is not registered or mDNS unregistration fails.
    pub fn unregister(&mut self) -> Result<(), AdvertiserError> {
        let fullname = self
            .service_fullname
            .take()
            .ok_or(AdvertiserError::NotRegistered)?;

        self.daemon.unregister(&fullname)?;

        tracing::info!(name = %fullname, "RAOP service unregistered");

        Ok(())
    }

    /// Update the status flags (e.g., mark as busy when streaming)
    ///
    /// This re-registers the service with updated TXT records.
    ///
    /// # Errors
    ///
    /// Returns error if re-registration fails.
    pub fn update_status(&mut self, status: ReceiverStatusFlags) -> Result<(), AdvertiserError> {
        {
            let mut current = self.status.blocking_write();
            *current = status;
        }

        // If registered, need to re-register to update TXT
        if self.service_fullname.is_some() {
            // Unregister then re-register with new TXT
            self.unregister()?;
            self.register()?;
        }

        Ok(())
    }

    /// Mark receiver as busy (streaming in progress)
    ///
    /// # Errors
    ///
    /// Returns error if status update fails.
    pub fn set_busy(&mut self, busy: bool) -> Result<(), AdvertiserError> {
        let current = *self.status.blocking_read();
        self.update_status(ReceiverStatusFlags { busy, ..current })
    }

    /// Get shared status handle for async status updates
    #[must_use]
    pub fn status_handle(&self) -> Arc<RwLock<ReceiverStatusFlags>> {
        self.status.clone()
    }
}

impl Drop for RaopAdvertiser {
    fn drop(&mut self) {
        // Best-effort unregister on drop
        if self.service_fullname.is_some() {
            let _ = self.unregister();
        }
    }
}

/// Commands for async advertiser control
#[derive(Debug)]
pub enum AdvertiserCommand {
    /// Update receiver status
    UpdateStatus(ReceiverStatusFlags),
    /// Shutdown the advertiser
    Shutdown,
}

/// Async-friendly RAOP advertiser
///
/// Wraps the synchronous mdns-sd advertiser in a background task
/// and provides async methods for control.
pub struct AsyncRaopAdvertiser {
    command_tx: mpsc::Sender<AdvertiserCommand>,
    status: Arc<RwLock<ReceiverStatusFlags>>,
    mac: [u8; 6],
    service_name: String,
}

impl AsyncRaopAdvertiser {
    /// Create and start the advertiser
    ///
    /// # Errors
    ///
    /// Returns error if advertiser creation fails (e.g. mDNS init or MAC retrieval).
    pub async fn start(mut config: AdvertiserConfig) -> Result<Self, AdvertiserError> {
        let (command_tx, mut command_rx) = mpsc::channel(16);

        let mac = if let Some(mac) = config.mac_override {
            Ok(mac)
        } else {
            tokio::task::spawn_blocking(get_device_mac)
                .await
                .map_err(|e| AdvertiserError::MacRetrievalFailed(e.to_string()))?
        }?;

        let service_name = format!("{}@{}", format_mac_for_service(&mac), config.name);
        let status = Arc::new(RwLock::new(ReceiverStatusFlags::default()));
        // let status_clone = status.clone();

        // Spawn blocking task for mdns-sd
        config.mac_override = Some(mac);

        tokio::task::spawn_blocking(move || {
            let mut advertiser = match RaopAdvertiser::new(config) {
                Ok(a) => a,
                Err(e) => {
                    tracing::error!("Failed to create advertiser: {}", e);
                    return;
                }
            };

            if let Err(e) = advertiser.register() {
                tracing::error!("Failed to register service: {}", e);
                return;
            }

            // Process commands until shutdown
            while let Some(cmd) = command_rx.blocking_recv() {
                match cmd {
                    AdvertiserCommand::UpdateStatus(new_status) => {
                        if let Err(e) = advertiser.update_status(new_status) {
                            tracing::warn!("Failed to update status: {}", e);
                        }
                    }
                    AdvertiserCommand::Shutdown => {
                        break;
                    }
                }
            }

            // Unregister on exit
            let _ = advertiser.unregister();
        });

        Ok(Self {
            command_tx,
            status,
            mac,
            service_name,
        })
    }

    /// Update the receiver status
    ///
    /// # Errors
    ///
    /// Returns error if advertiser loop has exited.
    pub async fn update_status(&self, status: ReceiverStatusFlags) -> Result<(), AdvertiserError> {
        {
            let mut current = self.status.write().await;
            *current = status;
        }

        self.command_tx
            .send(AdvertiserCommand::UpdateStatus(status))
            .await
            .map_err(|_| AdvertiserError::NotRegistered)
    }

    /// Mark as busy
    ///
    /// # Errors
    ///
    /// Returns error if status update fails.
    pub async fn set_busy(&self, busy: bool) -> Result<(), AdvertiserError> {
        let current = *self.status.read().await;
        self.update_status(ReceiverStatusFlags { busy, ..current })
            .await
    }

    /// Get the service name being advertised
    #[must_use]
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Get the MAC address
    #[must_use]
    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }

    /// Shutdown the advertiser
    pub async fn shutdown(self) {
        let _ = self.command_tx.send(AdvertiserCommand::Shutdown).await;
    }
}

/// Generic service advertiser that can be used for both
/// `AirPlay` 1 (RAOP) and `AirPlay` 2 receivers
pub struct ServiceAdvertiser {
    daemon: ServiceDaemon,
    registered_services: Arc<Mutex<HashMap<String, ServiceInfo>>>,
}

impl ServiceAdvertiser {
    /// Create a new service advertiser
    ///
    /// # Errors
    ///
    /// Returns error if mDNS daemon initialization fails.
    pub fn new() -> Result<Self, AdvertiserError> {
        let daemon = ServiceDaemon::new()?;

        Ok(Self {
            daemon,
            registered_services: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Register a service
    ///
    /// # Errors
    ///
    /// Returns error if service creation or registration fails.
    pub async fn register(
        &self,
        service_type: &str,
        name: &str,
        port: u16,
        txt_records: &[(String, String)],
    ) -> Result<String, AdvertiserError> {
        let hostname = Self::get_hostname();

        let service_info = ServiceInfo::new(
            service_type,
            name,
            &hostname,
            "",
            port,
            txt_records
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<String, String>>(),
        )
        .map_err(AdvertiserError::Mdns)?;

        let fullname = service_info.get_fullname().to_string();

        self.daemon.register(service_info.clone())?;

        self.registered_services
            .lock()
            .await
            .insert(fullname.clone(), service_info);

        Ok(fullname)
    }

    /// Unregister a service by fullname
    ///
    /// # Errors
    ///
    /// Returns error if mDNS unregistration fails.
    pub async fn unregister(&self, fullname: &str) -> Result<(), AdvertiserError> {
        if self
            .registered_services
            .lock()
            .await
            .remove(fullname)
            .is_some()
        {
            self.daemon.unregister(fullname)?;
        }
        Ok(())
    }

    /// Unregister all services
    ///
    /// # Errors
    ///
    /// Returns error if mDNS unregistration fails.
    pub async fn unregister_all(&self) -> Result<(), AdvertiserError> {
        // Collect all services and clear the map while holding the lock
        let services = {
            let mut guard = self.registered_services.lock().await;
            std::mem::take(&mut *guard)
        };

        // Iterate and unregister without holding the lock
        for (fullname, _) in services {
            self.daemon.unregister(&fullname)?;
        }

        Ok(())
    }

    fn get_hostname() -> String {
        hostname::get().map_or_else(
            |_| "airplay-receiver.local.".to_string(),
            |s| format!("{}.local.", s.to_string_lossy()),
        )
    }
}
