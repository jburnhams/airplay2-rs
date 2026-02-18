//! Example: Create a multi-room group

use std::time::Duration;

use airplay2::{GroupManager, scan};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let devices = scan(Duration::from_secs(3)).await?;
    if devices.len() < 2 {
        println!("Need at least 2 devices for multi-room example");
        return Ok(());
    }

    let manager = GroupManager::new();

    // Create group
    println!("Creating 'Party Group'...");
    let group_id = manager.create_group("Party Group").await;

    // Add devices
    for device in devices.iter().take(2) {
        println!("Adding {} to group...", device.name);
        manager
            .add_device_to_group(&group_id, device.clone())
            .await?;
    }

    // Set group volume
    println!("Setting group volume to 50%...");
    manager.set_group_volume(&group_id, 0.5.into()).await?;

    // In a real implementation, you would now use the group ID
    // to stream audio to the leader device, which syncs with followers.

    if let Some(group) = manager.get_group(&group_id).await {
        println!("Group created with {} members.", group.member_count());
    } else {
        println!("Group not found after creation.");
    }

    Ok(())
}
