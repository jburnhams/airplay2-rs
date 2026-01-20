use super::parser;
use crate::error::AirPlayError;
use crate::types::{AirPlayConfig, AirPlayDevice};
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

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
    config: AirPlayConfig,
}

impl DeviceBrowser {
    /// Create a new device browser
    #[must_use]
    pub fn new(config: AirPlayConfig) -> Self {
        Self { config }
    }

    /// Start browsing for devices
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon cannot be initialized.
    pub fn browse(self) -> Result<impl Stream<Item = DiscoveryEvent>, AirPlayError> {
        DeviceBrowserStream::new(self.config)
    }
}

/// Stream implementation for device discovery
struct DeviceBrowserStream {
    #[allow(dead_code)]
    // Config is stored for potential future use or to keep the API consistent,
    // even though mdns-sd configuration is limited.
    config: AirPlayConfig,
    mdns: mdns_sd::ServiceDaemon,
    stream: Box<dyn Stream<Item = mdns_sd::ServiceEvent> + Send + Unpin>,
    known_devices: HashMap<String, AirPlayDevice>,
    fullname_map: HashMap<String, String>,
}

impl DeviceBrowserStream {
    fn new(config: AirPlayConfig) -> Result<Self, AirPlayError> {
        let mdns = mdns_sd::ServiceDaemon::new().map_err(|e| AirPlayError::DiscoveryFailed {
            message: format!("Failed to create mDNS daemon: {e}"),
            source: None,
        })?;

        let receiver = mdns.browse(super::AIRPLAY_SERVICE_TYPE).map_err(|e| {
            AirPlayError::DiscoveryFailed {
                message: format!("Failed to browse: {e}"),
                source: None,
            }
        })?;

        // Convert receiver to stream and box it
        // mdns-sd receiver supports .stream() which returns a RecvStream
        let stream = Box::new(receiver.into_stream());

        Ok(Self {
            config,
            mdns,
            stream,
            known_devices: HashMap::new(),
            fullname_map: HashMap::new(),
        })
    }

    fn process_event(&mut self, event: mdns_sd::ServiceEvent) -> Option<DiscoveryEvent> {
        match event {
            mdns_sd::ServiceEvent::ServiceResolved(info) => self.handle_resolved(&info),
            mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => self.handle_removed(&fullname),
            _ => None,
        }
    }

    fn handle_resolved(&mut self, info: &mdns_sd::ServiceInfo) -> Option<DiscoveryEvent> {
        // Extract device info from service
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

        // Get device ID from TXT records
        let device_id = txt_records
            .get("deviceid")
            .or_else(|| txt_records.get("pk"))
            .cloned()
            .unwrap_or_else(|| name.clone());

        // Update map
        self.fullname_map.insert(name.clone(), device_id.clone());

        // Parse capabilities from features flag
        let capabilities = txt_records
            .get("features")
            .and_then(|f| parser::parse_features(f))
            .unwrap_or_default();

        // Get first resolved address
        let address = info.get_addresses().iter().next().copied()?;

        // Get friendly name
        let friendly_name = txt_records
            .get("model")
            .cloned()
            .or_else(|| {
                // Extract name from fullname (before first dot)
                name.split('.').next().map(ToString::to_string)
            })
            .unwrap_or_else(|| "AirPlay Device".to_string());

        let device = AirPlayDevice {
            id: device_id.clone(),
            name: friendly_name,
            model: txt_records.get("model").cloned(),
            address,
            port: info.get_port(),
            capabilities,
            txt_records,
        };

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
            self.known_devices.remove(&id);
            Some(DiscoveryEvent::Removed(id))
        } else {
            None
        }
    }
}

impl Stream for DeviceBrowserStream {
    type Item = DiscoveryEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let event = match Pin::new(&mut self.stream).poll_next(cx) {
                Poll::Ready(Some(event)) => event,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            };

            if let Some(discovery_event) = self.process_event(event) {
                return Poll::Ready(Some(discovery_event));
            }
        }
    }
}

impl Drop for DeviceBrowserStream {
    fn drop(&mut self) {
        // Stop browsing
        let _ = self.mdns.stop_browse(super::AIRPLAY_SERVICE_TYPE);
        let _ = self.mdns.shutdown();
    }
}
