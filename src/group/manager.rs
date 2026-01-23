//! Multi-room group management

use crate::control::volume::Volume;
use crate::error::AirPlayError;
use crate::types::AirPlayDevice;

use rand::Rng;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Unique identifier for a group
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GroupId(String);

impl GroupId {
    /// Create a new random group ID
    #[must_use]
    pub fn new() -> Self {
        let id: u128 = rand::thread_rng().r#gen();
        Self(format!("{id:032X}"))
    }

    /// Create from string
    #[must_use]
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get as string
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for GroupId {
    fn default() -> Self {
        Self::new()
    }
}

/// A device member of a group
#[derive(Debug, Clone)]
pub struct GroupMember {
    /// Device information
    pub device: AirPlayDevice,
    /// Individual volume (relative to group)
    pub volume: Volume,
    /// Is the group leader
    pub is_leader: bool,
    /// Connection state
    pub connected: bool,
}

/// A group of `AirPlay` devices
#[derive(Debug, Clone)]
pub struct DeviceGroup {
    /// Group identifier
    pub id: GroupId,
    /// Group name
    pub name: String,
    /// Group members
    members: Vec<GroupMember>,
    /// Leader device ID
    leader_id: Option<String>,
    /// Group volume
    volume: Volume,
}

impl DeviceGroup {
    /// Create a new empty group
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: GroupId::new(),
            name: name.into(),
            members: Vec::new(),
            leader_id: None,
            volume: Volume::DEFAULT,
        }
    }

    /// Create a group with a leader device
    #[must_use]
    pub fn with_leader(name: impl Into<String>, device: AirPlayDevice) -> Self {
        let leader_id = device.id.clone();
        let member = GroupMember {
            device,
            volume: Volume::MAX,
            is_leader: true,
            connected: false,
        };

        Self {
            id: GroupId::new(),
            name: name.into(),
            members: vec![member],
            leader_id: Some(leader_id),
            volume: Volume::DEFAULT,
        }
    }

    /// Add a device to the group
    pub fn add_member(&mut self, device: AirPlayDevice) {
        if self.members.iter().any(|m| m.device.id == device.id) {
            return; // Already a member
        }

        let is_leader = self.members.is_empty();
        if is_leader {
            self.leader_id = Some(device.id.clone());
        }

        self.members.push(GroupMember {
            device,
            volume: Volume::MAX,
            is_leader,
            connected: false,
        });
    }

    /// Remove a device from the group
    pub fn remove_member(&mut self, device_id: &str) -> Option<GroupMember> {
        let pos = self.members.iter().position(|m| m.device.id == device_id)?;
        let member = self.members.remove(pos);

        // If leader was removed, promote another
        if member.is_leader && !self.members.is_empty() {
            self.members[0].is_leader = true;
            self.leader_id = Some(self.members[0].device.id.clone());
        } else if self.members.is_empty() {
            self.leader_id = None;
        }

        Some(member)
    }

    /// Get the group leader
    #[must_use]
    pub fn leader(&self) -> Option<&GroupMember> {
        self.members.iter().find(|m| m.is_leader)
    }

    /// Get all members
    #[must_use]
    pub fn members(&self) -> &[GroupMember] {
        &self.members
    }

    /// Get member by device ID
    #[must_use]
    pub fn member(&self, device_id: &str) -> Option<&GroupMember> {
        self.members.iter().find(|m| m.device.id == device_id)
    }

    /// Get mutable member
    pub fn member_mut(&mut self, device_id: &str) -> Option<&mut GroupMember> {
        self.members.iter_mut().find(|m| m.device.id == device_id)
    }

    /// Set individual device volume
    pub fn set_member_volume(&mut self, device_id: &str, volume: Volume) {
        if let Some(member) = self.member_mut(device_id) {
            member.volume = volume;
        }
    }

    /// Get group volume
    #[must_use]
    pub fn volume(&self) -> Volume {
        self.volume
    }

    /// Set group volume
    pub fn set_volume(&mut self, volume: Volume) {
        self.volume = volume;
    }

    /// Get effective volume for a device
    #[must_use]
    pub fn effective_volume(&self, device_id: &str) -> Volume {
        let member_vol = self.member(device_id).map_or(Volume::MAX, |m| m.volume);

        Volume::new(self.volume.as_f32() * member_vol.as_f32())
    }

    /// Check if group is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Get member count
    #[must_use]
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Check if all members are connected
    #[must_use]
    pub fn all_connected(&self) -> bool {
        self.members.iter().all(|m| m.connected)
    }

    /// Get connected member count
    #[must_use]
    pub fn connected_count(&self) -> usize {
        self.members.iter().filter(|m| m.connected).count()
    }
}

/// Manager for device groups
pub struct GroupManager {
    /// Active groups
    groups: RwLock<HashMap<GroupId, DeviceGroup>>,
    /// Device to group mapping
    device_groups: RwLock<HashMap<String, GroupId>>,
}

impl GroupManager {
    /// Create a new group manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            groups: RwLock::new(HashMap::new()),
            device_groups: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new group
    pub async fn create_group(&self, name: impl Into<String>) -> GroupId {
        let group = DeviceGroup::new(name);
        let id = group.id.clone();
        self.groups.write().await.insert(id.clone(), group);
        id
    }

    /// Create a group with initial devices
    ///
    /// # Errors
    ///
    /// Returns error if any device is already in a group
    pub async fn create_group_with_devices(
        &self,
        name: impl Into<String>,
        devices: Vec<AirPlayDevice>,
    ) -> Result<GroupId, AirPlayError> {
        let mut groups = self.groups.write().await;
        let mut device_groups = self.device_groups.write().await;

        // Check availability
        for device in &devices {
            if device_groups.contains_key(&device.id) {
                return Err(AirPlayError::InvalidState {
                    message: format!("Device {} is already in a group", device.id),
                    current_state: "grouped".to_string(),
                });
            }
        }

        let mut group = DeviceGroup::new(name);

        for device in devices {
            let device_id = device.id.clone();
            group.add_member(device);
            device_groups.insert(device_id, group.id.clone());
        }

        let id = group.id.clone();
        groups.insert(id.clone(), group);
        Ok(id)
    }

    /// Delete a group
    pub async fn delete_group(&self, id: &GroupId) -> Option<DeviceGroup> {
        let group = self.groups.write().await.remove(id)?;

        // Remove device mappings
        let mut device_groups = self.device_groups.write().await;
        for member in &group.members {
            device_groups.remove(&member.device.id);
        }

        Some(group)
    }

    /// Get a group by ID
    pub async fn get_group(&self, id: &GroupId) -> Option<DeviceGroup> {
        self.groups.read().await.get(id).cloned()
    }

    /// Get all groups
    pub async fn all_groups(&self) -> Vec<DeviceGroup> {
        self.groups.read().await.values().cloned().collect()
    }

    /// Find group containing a device
    pub async fn find_device_group(&self, device_id: &str) -> Option<GroupId> {
        self.device_groups.read().await.get(device_id).cloned()
    }

    /// Add device to a group
    ///
    /// # Errors
    ///
    /// Returns error if device is already in a group or group is not found
    pub async fn add_device_to_group(
        &self,
        group_id: &GroupId,
        device: AirPlayDevice,
    ) -> Result<(), AirPlayError> {
        let device_id = device.id.clone();

        // Lock order: groups -> device_groups
        let mut groups = self.groups.write().await;
        let mut device_groups = self.device_groups.write().await;

        // Check if device is already in a group
        if device_groups.contains_key(&device_id) {
            return Err(AirPlayError::InvalidState {
                message: "Device is already in a group".to_string(),
                current_state: "grouped".to_string(),
            });
        }

        // Add to group
        let group = groups
            .get_mut(group_id)
            .ok_or(AirPlayError::DeviceNotFound {
                device_id: group_id.as_str().to_string(),
            })?;

        group.add_member(device);
        device_groups.insert(device_id, group_id.clone());

        Ok(())
    }

    /// Remove device from its group
    ///
    /// # Errors
    ///
    /// Returns error if internal state is inconsistent (rare)
    pub async fn remove_device_from_group(&self, device_id: &str) -> Result<(), AirPlayError> {
        let group_id = self.device_groups.write().await.remove(device_id);

        if let Some(group_id) = group_id {
            let mut groups = self.groups.write().await;
            if let Some(group) = groups.get_mut(&group_id) {
                group.remove_member(device_id);

                // Remove group if empty
                if group.is_empty() {
                    groups.remove(&group_id);
                }
            }
        }

        Ok(())
    }

    /// Set group volume
    ///
    /// # Errors
    ///
    /// Returns error if group not found
    pub async fn set_group_volume(
        &self,
        group_id: &GroupId,
        volume: Volume,
    ) -> Result<(), AirPlayError> {
        let mut groups = self.groups.write().await;
        let group = groups
            .get_mut(group_id)
            .ok_or(AirPlayError::DeviceNotFound {
                device_id: group_id.as_str().to_string(),
            })?;

        group.set_volume(volume);
        Ok(())
    }

    /// Set member volume
    ///
    /// # Errors
    ///
    /// Returns error if group not found
    pub async fn set_member_volume(
        &self,
        group_id: &GroupId,
        device_id: &str,
        volume: Volume,
    ) -> Result<(), AirPlayError> {
        let mut groups = self.groups.write().await;
        let group = groups
            .get_mut(group_id)
            .ok_or(AirPlayError::DeviceNotFound {
                device_id: group_id.as_str().to_string(),
            })?;

        group.set_member_volume(device_id, volume);
        Ok(())
    }
}

impl Default for GroupManager {
    fn default() -> Self {
        Self::new()
    }
}
