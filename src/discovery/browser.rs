use super::parser;
use super::raop;
use crate::error::AirPlayError;
use crate::types::{AirPlayConfig, AirPlayDevice, DeviceCapabilities, RaopCapabilities};
use futures::{Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// Extended discovery options for both `AirPlay` 1 and 2
#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    /// Discover `AirPlay` 2 devices (_airplay._tcp)
    pub discover_airplay2: bool,
    /// Discover `AirPlay` 1/RAOP devices (_raop._tcp)
    pub discover_raop: bool,
    /// Timeout for discovery scan (not used in continuous browse)
    pub timeout: Duration,
    /// Filter by device capabilities
    pub filter: Option<DeviceFilter>,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            discover_airplay2: true,
            discover_raop: true,
            timeout: Duration::from_secs(5),
            filter: None,
        }
    }
}

/// Device filter criteria
#[derive(Debug, Clone, Default)]
pub struct DeviceFilter {
    /// Require audio support
    pub audio_only: bool,
    /// Exclude password-protected devices
    pub exclude_password_protected: bool,
}

/// Discovery events
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new device was discovered
    Added(AirPlayDevice),
    /// A device was removed/went offline
    Removed(String),
    /// Device information was updated
    Updated(AirPlayDevice),
}

/// mDNS browser for discovering `AirPlay` devices
pub struct DeviceBrowser {
    options: DiscoveryOptions,
}

impl DeviceBrowser {
    /// Create a new device browser with default config (`AirPlay` 2 only for backward compat?)
    /// or should it default to both?
    /// The original `new` took `AirPlayConfig`.
    #[must_use]
    pub fn new(config: &AirPlayConfig) -> Self {
        // Map AirPlayConfig to DiscoveryOptions if possible, or use defaults
        // AirPlayConfig doesn't have specific discovery flags, so we assume default (both).
        Self {
            options: DiscoveryOptions {
                timeout: config.discovery_timeout,
                ..Default::default()
            },
        }
    }

    /// Create with specific options
    #[must_use]
    pub fn with_options(options: DiscoveryOptions) -> Self {
        Self { options }
    }

    /// Start browsing for devices
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon cannot be initialized.
    pub fn browse(self) -> Result<impl Stream<Item = DiscoveryEvent>, AirPlayError> {
        DeviceBrowserStream::new(self.options)
    }
}

/// Stream implementation for device discovery
struct DeviceBrowserStream {
    options: DiscoveryOptions,
    mdns: mdns_sd::ServiceDaemon,
    // Stream of events from all browsers
    stream: Pin<Box<dyn Stream<Item = (String, mdns_sd::ServiceEvent)> + Send>>,
    known_devices: HashMap<String, AirPlayDevice>,
    // Map full service name to device ID
    fullname_map: HashMap<String, String>,
}

impl DeviceBrowserStream {
    fn new(options: DiscoveryOptions) -> Result<Self, AirPlayError> {
        let mdns = mdns_sd::ServiceDaemon::new().map_err(|e| AirPlayError::DiscoveryFailed {
            message: format!("Failed to create mDNS daemon: {e}"),
            source: None,
        })?;

        let mut streams = Vec::new();

        if options.discover_airplay2 {
            let receiver = mdns.browse(super::AIRPLAY_SERVICE_TYPE).map_err(|e| {
                AirPlayError::DiscoveryFailed {
                    message: format!("Failed to browse AirPlay 2: {e}"),
                    source: None,
                }
            })?;
            // Tag events with service type
            let s = receiver
                .into_stream()
                .map(|e| (super::AIRPLAY_SERVICE_TYPE.to_string(), e));
            // Box::new(s) is Unpin if s is Unpin. map stream is Unpin if inner is Unpin.
            // receiver.into_stream() returns RecvStream which is Unpin.
            streams.push(Box::new(s) as Box<dyn Stream<Item = _> + Send + Unpin>);
        }

        if options.discover_raop {
            let receiver = mdns.browse(super::RAOP_SERVICE_TYPE).map_err(|e| {
                AirPlayError::DiscoveryFailed {
                    message: format!("Failed to browse RAOP: {e}"),
                    source: None,
                }
            })?;
            let s = receiver
                .into_stream()
                .map(|e| (super::RAOP_SERVICE_TYPE.to_string(), e));
            streams.push(Box::new(s) as Box<dyn Stream<Item = _> + Send + Unpin>);
        }

        let stream = futures::stream::select_all(streams);

        Ok(Self {
            options,
            mdns,
            stream: Box::pin(stream),
            known_devices: HashMap::new(),
            fullname_map: HashMap::new(),
        })
    }

    fn process_event(
        &mut self,
        service_type: &str,
        event: mdns_sd::ServiceEvent,
    ) -> Option<DiscoveryEvent> {
        match event {
            mdns_sd::ServiceEvent::ServiceResolved(info) => {
                self.handle_resolved(service_type, &info)
            }
            mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => self.handle_removed(&fullname),
            _ => None,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_resolved(
        &mut self,
        service_type: &str,
        info: &mdns_sd::ResolvedService,
    ) -> Option<DiscoveryEvent> {
        let name = info.get_fullname().to_string();

        // Parse TXT records
        let txt_records: HashMap<String, String> = info
            .get_properties()
            .iter()
            .map(|prop| {
                let key = prop.key().to_string();
                (key, prop.val_str().to_string())
            })
            .collect();

        // Determine Device ID
        let device_id = if service_type == super::RAOP_SERVICE_TYPE {
            // For RAOP, parse from service name: MAC@Name
            // Extract MAC and format it to match standard ID format (if needed)
            // Assuming standard ID format is MAC address with colons?
            // AirPlay 2 usually sends MAC.
            if let Some((mac, _)) = raop::parse_raop_service_name(info.get_fullname()) {
                raop::format_mac_address(&mac)
            } else {
                // Fallback
                name.clone()
            }
        } else {
            // AirPlay 2
            txt_records
                .get("deviceid")
                .or_else(|| txt_records.get("pk"))
                .cloned()
                .unwrap_or_else(|| name.clone())
        };

        // Update map
        self.fullname_map.insert(name.clone(), device_id.clone());

        // Get resolved addresses
        let addresses: Vec<std::net::IpAddr> = info
            .get_addresses()
            .iter()
            .map(|ip| {
                // Handle ScopedIp from mdns-sd 0.17
                match ip {
                    mdns_sd::ScopedIp::V4(scoped) => std::net::IpAddr::V4(*scoped.addr()),
                    mdns_sd::ScopedIp::V6(scoped) => std::net::IpAddr::V6(*scoped.addr()),
                    _ => unreachable!("Unknown ScopedIp variant"),
                }
            })
            .collect();
        if addresses.is_empty() {
            return None;
        }

        // Get friendly name
        let friendly_name = if service_type == super::RAOP_SERVICE_TYPE {
            raop::parse_raop_service_name(info.get_fullname())
                .map_or_else(|| "Unknown RAOP Device".to_string(), |(_, n)| n)
        } else {
            // For AirPlay 2 devices, the service instance name (before the service type)
            // is the user-assigned friendly name (e.g., "Kitchen" from "Kitchen._airplay._tcp.local.")
            name.split('.')
                .next()
                .filter(|n| !n.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    txt_records
                        .get("model")
                        .cloned()
                        .unwrap_or_else(|| "AirPlay Device".to_string())
                })
        };

        // Create or update device
        let mut device = self
            .known_devices
            .get(&device_id)
            .cloned()
            .unwrap_or_else(|| {
                // Initialize new device
                AirPlayDevice {
                    id: device_id.clone(),
                    name: friendly_name.clone(),
                    model: txt_records.get("model").cloned(),
                    addresses: addresses.clone(),
                    port: 0, // Will be set below
                    capabilities: DeviceCapabilities::default(),
                    raop_port: None,
                    raop_capabilities: None,
                    txt_records: HashMap::new(),
                }
            });

        // Update name/model if missing or if this is the "better" source?
        // Usually assume AirPlay 2 info is better if available, but for now just update.
        if device.name == "AirPlay Device" || device.name == "Unknown RAOP Device" {
            device.name = friendly_name;
        }
        if device.model.is_none() {
            device.model = txt_records.get("model").cloned();
        }

        // Merge addresses (deduplicate?)
        for addr in addresses {
            if !device.addresses.contains(&addr) {
                device.addresses.push(addr);
            }
        }

        // Merge TXT records
        device.txt_records.extend(txt_records.clone());

        // Update protocol specific info
        if service_type == super::AIRPLAY_SERVICE_TYPE {
            device.port = info.get_port();
            if let Some(features) = txt_records.get("features") {
                if let Some(caps) = parser::parse_features(features) {
                    device.capabilities = caps;
                }
            }
        } else if service_type == super::RAOP_SERVICE_TYPE {
            device.raop_port = Some(info.get_port());
            device.raop_capabilities = Some(RaopCapabilities::from_txt_records(&txt_records));

            // If only RAOP, set main port to RAOP port for convenience?
            // But main port is u16 (mandatory).
            if device.port == 0 {
                device.port = info.get_port();
            }
        }

        // Filter check
        if let Some(filter) = &self.options.filter {
            if filter.audio_only
                && !device.capabilities.supports_audio
                && device.raop_capabilities.is_none()
            {
                return None;
            }
            // Add more filter checks...
        }

        // Check if this is new or updated
        let event = if self.known_devices.contains_key(&device_id) {
            DiscoveryEvent::Updated(device.clone())
        } else {
            DiscoveryEvent::Added(device.clone())
        };

        self.known_devices.insert(device_id, device);

        Some(event)
    }

    fn handle_removed(&mut self, fullname: &str) -> Option<DiscoveryEvent> {
        // Find device ID by fullname
        let device_id = self.fullname_map.get(fullname).cloned();

        if let Some(id) = device_id {
            self.fullname_map.remove(fullname);
            // We only remove the device if ALL services are gone?
            // Currently simplified: if any service is removed, we send removed event?
            // No, we should probably check if other services map to the same ID.
            // But for now, if we lose a service, we might assume device is gone or just update?
            // If we remove it from known_devices, it's GONE.

            // Better logic: Check if other fullnames map to this ID.
            let has_other_services = self.fullname_map.values().any(|v| v == &id);

            if has_other_services {
                // Maybe update?
                // For now, ignoring partial removal (e.g. RAOP gone but AirPlay 2 stays).
                // Ideally we should update the device to reflect lost capabilities.
                None
            } else {
                self.known_devices.remove(&id);
                Some(DiscoveryEvent::Removed(id))
            }
        } else {
            None
        }
    }
}

impl Stream for DeviceBrowserStream {
    type Item = DiscoveryEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let (service_type, event) = match self.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(item)) => item,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            };

            if let Some(discovery_event) = self.process_event(&service_type, event) {
                return Poll::Ready(Some(discovery_event));
            }
        }
    }
}

impl Drop for DeviceBrowserStream {
    fn drop(&mut self) {
        // Stop browsing
        if self.options.discover_airplay2 {
            let _ = self.mdns.stop_browse(super::AIRPLAY_SERVICE_TYPE);
        }
        if self.options.discover_raop {
            let _ = self.mdns.stop_browse(super::RAOP_SERVICE_TYPE);
        }
        let _ = self.mdns.shutdown();
    }
}
