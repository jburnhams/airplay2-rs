# Section 18: Volume Control

## Dependencies
- **Section 05**: RTSP Protocol (must be complete)
- **Section 10**: Connection Management (must be complete)

## Overview

This section implements volume control for AirPlay devices including:
- Volume adjustment
- Mute/unmute
- Volume synchronization with device
- Multi-room volume control

## Objectives

- Implement volume control commands
- Handle device volume feedback
- Support per-device volumes in groups
- Provide volume normalization

---

## Tasks

### 18.1 Volume Controller

- [ ] **18.1.1** Implement volume control

**File:** `src/control/volume.rs`

```rust
//! Volume control for AirPlay devices

use crate::connection::ConnectionManager;
use crate::protocol::rtsp::Method;
use crate::error::AirPlayError;

use std::sync::Arc;
use tokio::sync::RwLock;

/// Volume level (0.0 = silent, 1.0 = max)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volume(f32);

impl Volume {
    /// Minimum volume (silent)
    pub const MIN: Self = Self(0.0);
    /// Maximum volume
    pub const MAX: Self = Self(1.0);
    /// Default volume (75%)
    pub const DEFAULT: Self = Self(0.75);

    /// Create a new volume level
    pub fn new(level: f32) -> Self {
        Self(level.clamp(0.0, 1.0))
    }

    /// Get as f32 (0.0 - 1.0)
    pub fn as_f32(&self) -> f32 {
        self.0
    }

    /// Get as percentage (0 - 100)
    pub fn as_percent(&self) -> u8 {
        (self.0 * 100.0).round() as u8
    }

    /// Create from percentage
    pub fn from_percent(percent: u8) -> Self {
        Self::new(percent as f32 / 100.0)
    }

    /// Convert to AirPlay dB scale (-144 to 0)
    pub fn to_db(&self) -> f32 {
        if self.0 <= 0.0 {
            -144.0
        } else {
            // Logarithmic scale
            20.0 * self.0.log10()
        }
    }

    /// Create from AirPlay dB scale
    pub fn from_db(db: f32) -> Self {
        if db <= -144.0 {
            Self::MIN
        } else {
            Self::new(10.0_f32.powf(db / 20.0))
        }
    }

    /// Check if effectively silent
    pub fn is_silent(&self) -> bool {
        self.0 < 0.001
    }

    /// Check if at maximum
    pub fn is_max(&self) -> bool {
        self.0 >= 0.999
    }
}

impl Default for Volume {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<f32> for Volume {
    fn from(v: f32) -> Self {
        Self::new(v)
    }
}

/// Volume controller
pub struct VolumeController {
    /// Connection manager
    connection: Arc<ConnectionManager>,
    /// Current volume
    volume: RwLock<Volume>,
    /// Mute state
    muted: RwLock<bool>,
    /// Volume before mute (for unmute)
    pre_mute_volume: RwLock<Volume>,
}

impl VolumeController {
    /// Create a new volume controller
    pub fn new(connection: Arc<ConnectionManager>) -> Self {
        Self {
            connection,
            volume: RwLock::new(Volume::DEFAULT),
            muted: RwLock::new(false),
            pre_mute_volume: RwLock::new(Volume::DEFAULT),
        }
    }

    /// Get current volume
    pub async fn get(&self) -> Volume {
        *self.volume.read().await
    }

    /// Set volume
    pub async fn set(&self, volume: Volume) -> Result<(), AirPlayError> {
        // Send to device
        self.send_volume(volume).await?;

        // Update local state
        *self.volume.write().await = volume;

        // Unmute if setting non-zero volume
        if !volume.is_silent() {
            *self.muted.write().await = false;
        }

        Ok(())
    }

    /// Set volume from percentage
    pub async fn set_percent(&self, percent: u8) -> Result<(), AirPlayError> {
        self.set(Volume::from_percent(percent)).await
    }

    /// Increase volume by amount
    pub async fn increase(&self, amount: f32) -> Result<Volume, AirPlayError> {
        let current = self.get().await;
        let new_volume = Volume::new(current.as_f32() + amount);
        self.set(new_volume).await?;
        Ok(new_volume)
    }

    /// Decrease volume by amount
    pub async fn decrease(&self, amount: f32) -> Result<Volume, AirPlayError> {
        let current = self.get().await;
        let new_volume = Volume::new(current.as_f32() - amount);
        self.set(new_volume).await?;
        Ok(new_volume)
    }

    /// Step volume up (by 5%)
    pub async fn step_up(&self) -> Result<Volume, AirPlayError> {
        self.increase(0.05).await
    }

    /// Step volume down (by 5%)
    pub async fn step_down(&self) -> Result<Volume, AirPlayError> {
        self.decrease(0.05).await
    }

    /// Check if muted
    pub async fn is_muted(&self) -> bool {
        *self.muted.read().await
    }

    /// Mute
    pub async fn mute(&self) -> Result<(), AirPlayError> {
        if !self.is_muted().await {
            // Save current volume
            *self.pre_mute_volume.write().await = self.get().await;

            // Set to silent
            self.send_volume(Volume::MIN).await?;
            *self.muted.write().await = true;
        }
        Ok(())
    }

    /// Unmute
    pub async fn unmute(&self) -> Result<(), AirPlayError> {
        if self.is_muted().await {
            let volume = *self.pre_mute_volume.read().await;
            self.send_volume(volume).await?;
            *self.volume.write().await = volume;
            *self.muted.write().await = false;
        }
        Ok(())
    }

    /// Toggle mute
    pub async fn toggle_mute(&self) -> Result<bool, AirPlayError> {
        if self.is_muted().await {
            self.unmute().await?;
            Ok(false)
        } else {
            self.mute().await?;
            Ok(true)
        }
    }

    /// Sync volume from device
    pub async fn sync_from_device(&self) -> Result<Volume, AirPlayError> {
        let volume = self.get_device_volume().await?;
        *self.volume.write().await = volume;
        *self.muted.write().await = volume.is_silent();
        Ok(volume)
    }

    /// Send volume to device
    async fn send_volume(&self, volume: Volume) -> Result<(), AirPlayError> {
        // AirPlay uses dB scale in the volume parameter
        let db = volume.to_db();

        // Format: "volume: -30.000000\r\n"
        let body = format!("volume: {:.6}\r\n", db);

        // TODO: Send SET_PARAMETER with volume
        // self.connection.send_set_parameter("text/parameters", body.as_bytes()).await?;

        Ok(())
    }

    /// Get volume from device
    async fn get_device_volume(&self) -> Result<Volume, AirPlayError> {
        // TODO: Send GET_PARAMETER and parse response
        Ok(Volume::DEFAULT)
    }
}

/// Multi-device volume control
pub struct GroupVolumeController {
    /// Device controllers
    devices: Vec<DeviceVolume>,
    /// Master volume
    master_volume: Volume,
}

/// Volume for a single device in a group
pub struct DeviceVolume {
    /// Device ID
    pub device_id: String,
    /// Individual volume multiplier
    pub volume: Volume,
    /// Controller
    controller: Arc<VolumeController>,
}

impl GroupVolumeController {
    /// Create a new group volume controller
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            master_volume: Volume::DEFAULT,
        }
    }

    /// Add a device
    pub fn add_device(&mut self, device_id: String, controller: Arc<VolumeController>) {
        self.devices.push(DeviceVolume {
            device_id,
            volume: Volume::MAX, // Full relative volume
            controller,
        });
    }

    /// Remove a device
    pub fn remove_device(&mut self, device_id: &str) {
        self.devices.retain(|d| d.device_id != device_id);
    }

    /// Set master volume (applies to all devices)
    pub async fn set_master_volume(&mut self, volume: Volume) -> Result<(), AirPlayError> {
        self.master_volume = volume;
        self.apply_volumes().await
    }

    /// Set individual device volume (relative to master)
    pub async fn set_device_volume(
        &mut self,
        device_id: &str,
        volume: Volume,
    ) -> Result<(), AirPlayError> {
        if let Some(device) = self.devices.iter_mut().find(|d| d.device_id == device_id) {
            device.volume = volume;
        }
        self.apply_volumes().await
    }

    /// Apply volumes to all devices
    async fn apply_volumes(&self) -> Result<(), AirPlayError> {
        for device in &self.devices {
            let effective = Volume::new(self.master_volume.as_f32() * device.volume.as_f32());
            device.controller.set(effective).await?;
        }
        Ok(())
    }

    /// Mute all devices
    pub async fn mute_all(&self) -> Result<(), AirPlayError> {
        for device in &self.devices {
            device.controller.mute().await?;
        }
        Ok(())
    }

    /// Unmute all devices
    pub async fn unmute_all(&self) -> Result<(), AirPlayError> {
        for device in &self.devices {
            device.controller.unmute().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_percent() {
        let vol = Volume::from_percent(50);
        assert_eq!(vol.as_percent(), 50);

        let vol = Volume::from_percent(100);
        assert_eq!(vol.as_f32(), 1.0);

        let vol = Volume::from_percent(0);
        assert_eq!(vol.as_f32(), 0.0);
    }

    #[test]
    fn test_volume_db() {
        let vol = Volume::MAX;
        assert_eq!(vol.to_db(), 0.0);

        let vol = Volume::MIN;
        assert_eq!(vol.to_db(), -144.0);

        // Test roundtrip
        let vol = Volume::new(0.5);
        let db = vol.to_db();
        let recovered = Volume::from_db(db);
        assert!((vol.as_f32() - recovered.as_f32()).abs() < 0.001);
    }

    #[test]
    fn test_volume_clamping() {
        let vol = Volume::new(1.5);
        assert_eq!(vol.as_f32(), 1.0);

        let vol = Volume::new(-0.5);
        assert_eq!(vol.as_f32(), 0.0);
    }

    #[test]
    fn test_is_silent() {
        assert!(Volume::MIN.is_silent());
        assert!(Volume::new(0.0005).is_silent());
        assert!(!Volume::new(0.01).is_silent());
    }
}
```

---

## Acceptance Criteria

- [ ] Volume set/get works correctly
- [ ] Mute/unmute works correctly
- [ ] dB conversion is accurate
- [ ] Volume sync from device works
- [ ] Multi-device volume works
- [ ] All unit tests pass

---

## Notes

- AirPlay uses dB scale (-144 to 0)
- Some devices may have different volume ranges
- Consider adding volume curves (linear vs logarithmic)
- May need to debounce rapid volume changes
