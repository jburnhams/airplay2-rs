//! Network condition simulation for testing

use rand::Rng;
use std::time::Duration;

/// Network condition simulator
#[derive(Clone, Debug)]
pub struct NetworkSimulator {
    /// Packet loss probability (0.0 to 1.0)
    pub loss_rate: f64,
    /// Jitter range (max delay added)
    pub jitter_ms: u32,
    /// Base delay added to all packets
    pub delay_ms: u32,
    /// Probability of reordering
    pub reorder_rate: f64,
}

impl NetworkSimulator {
    /// Perfect network (no issues)
    #[must_use]
    pub fn perfect() -> Self {
        Self {
            loss_rate: 0.0,
            jitter_ms: 0,
            delay_ms: 0,
            reorder_rate: 0.0,
        }
    }

    /// Good `WiFi` conditions
    #[must_use]
    pub fn good_wifi() -> Self {
        Self {
            loss_rate: 0.001,
            jitter_ms: 5,
            delay_ms: 2,
            reorder_rate: 0.001,
        }
    }

    /// Moderate `WiFi` conditions
    #[must_use]
    pub fn moderate_wifi() -> Self {
        Self {
            loss_rate: 0.01,
            jitter_ms: 20,
            delay_ms: 10,
            reorder_rate: 0.01,
        }
    }

    /// Poor `WiFi` conditions
    #[must_use]
    pub fn poor_wifi() -> Self {
        Self {
            loss_rate: 0.05,
            jitter_ms: 50,
            delay_ms: 30,
            reorder_rate: 0.05,
        }
    }

    /// Very poor conditions (stress test)
    #[must_use]
    pub fn stress_test() -> Self {
        Self {
            loss_rate: 0.10,
            jitter_ms: 100,
            delay_ms: 50,
            reorder_rate: 0.10,
        }
    }

    /// Should this packet be dropped?
    #[must_use]
    pub fn should_drop(&self) -> bool {
        if self.loss_rate <= 0.0 {
            return false;
        }
        rand::thread_rng().gen_bool(self.loss_rate)
    }

    /// Get delay for this packet
    #[must_use]
    pub fn get_delay(&self) -> Duration {
        let jitter: u32 = if self.jitter_ms > 0 {
            rand::thread_rng().gen_range(0..self.jitter_ms)
        } else {
            0
        };

        Duration::from_millis(u64::from(self.delay_ms + jitter))
    }

    /// Should this packet be reordered?
    #[must_use]
    pub fn should_reorder(&self) -> bool {
        if self.reorder_rate <= 0.0 {
            return false;
        }
        rand::thread_rng().gen_bool(self.reorder_rate)
    }
}
