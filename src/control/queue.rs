//! Playback queue management

use crate::types::{QueueItem, QueueItemId, TrackInfo};
use std::collections::VecDeque;

/// Playback queue
#[derive(Debug)]
pub struct PlaybackQueue {
    /// Queue items
    items: Vec<QueueItem>,
    /// Current playing index
    current_index: Option<usize>,
    /// Playback history (for previous)
    history: VecDeque<QueueItemId>,
    /// Maximum history size
    max_history: usize,
    /// Shuffle order (indices into items)
    shuffle_order: Option<Vec<usize>>,
    /// Current position in shuffle
    shuffle_position: usize,
}

impl PlaybackQueue {
    /// Create an empty queue
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current_index: None,
            history: VecDeque::new(),
            max_history: 100,
            shuffle_order: None,
            shuffle_position: 0,
        }
    }

    /// Add a track to the end of the queue
    pub fn add(&mut self, track: TrackInfo) -> QueueItemId {
        let position = self.items.len();
        let item = QueueItem::new(track, position);
        let id = item.id;
        self.items.push(item);

        // Update shuffle order if shuffled
        if let Some(ref mut order) = self.shuffle_order {
            order.push(position);
        }

        id
    }

    /// Insert a track at a specific position
    pub fn insert(&mut self, index: usize, track: TrackInfo) -> QueueItemId {
        let position = self.items.len();
        let item = QueueItem::new(track, position);
        let id = item.id;

        let insert_at = index.min(self.items.len());
        self.items.insert(insert_at, item);

        // Update current index if needed
        if let Some(current) = self.current_index {
            if insert_at <= current {
                self.current_index = Some(current + 1);
            }
        }

        // Update shuffle order if shuffled
        if let Some(ref mut order) = self.shuffle_order {
            // Shift indices
            for idx in order.iter_mut() {
                if *idx >= insert_at {
                    *idx += 1;
                }
            }
            // Add new item to order
            order.push(insert_at);
        }

        id
    }

    /// Add a track to play next
    pub fn add_next(&mut self, track: TrackInfo) -> QueueItemId {
        let insert_at = self.current_index.map_or(0, |i| i + 1);
        self.insert(insert_at, track)
    }

    /// Remove a track by ID
    pub fn remove(&mut self, id: QueueItemId) -> Option<QueueItem> {
        let index = self.items.iter().position(|item| item.id == id)?;
        let item = self.items.remove(index);

        // Update current index
        if let Some(current) = self.current_index {
            if index < current {
                self.current_index = Some(current - 1);
            } else if index == current {
                // Current track was removed
                if self.items.is_empty() {
                    self.current_index = None;
                } else {
                    self.current_index = Some(current.min(self.items.len() - 1));
                }
            }
        }

        // Remove from shuffle order
        if let Some(ref mut order) = self.shuffle_order {
            order.retain(|&i| i != index);
            // Adjust indices
            for i in order.iter_mut() {
                if *i > index {
                    *i -= 1;
                }
            }
        }

        Some(item)
    }

    /// Move a track to a new position
    pub fn move_track(&mut self, from: usize, to: usize) {
        if from >= self.items.len() || to >= self.items.len() {
            return;
        }

        let item = self.items.remove(from);
        self.items.insert(to, item);

        // Update current index
        if let Some(current) = self.current_index {
            self.current_index = Some(if current == from {
                to
            } else if from < current && to >= current {
                current - 1
            } else if from > current && to <= current {
                current + 1
            } else {
                current
            });
        }

        // Update shuffle order
        if let Some(ref mut order) = self.shuffle_order {
            for idx in order.iter_mut() {
                if *idx == from {
                    *idx = to;
                } else if from < to {
                    if *idx > from && *idx <= to {
                        *idx -= 1;
                    }
                } else if *idx >= to && *idx < from {
                    *idx += 1;
                }
            }
        }
    }

    /// Clear the queue
    pub fn clear(&mut self) {
        self.items.clear();
        self.current_index = None;
        self.shuffle_order = None;
        self.shuffle_position = 0;
    }

    /// Get the current track
    #[must_use]
    pub fn current(&self) -> Option<&QueueItem> {
        self.current_index.and_then(|i| self.items.get(i))
    }

    /// Get the current index
    #[must_use]
    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    /// Set current index
    pub fn set_current(&mut self, index: usize) -> bool {
        if index < self.items.len() {
            // Add current to history before changing
            if let Some(current) = self.current() {
                self.add_to_history(current.id);
            }
            self.current_index = Some(index);
            true
        } else {
            false
        }
    }

    /// Skip to specific track by ID
    pub fn skip_to(&mut self, id: QueueItemId) -> bool {
        if let Some(index) = self.items.iter().position(|item| item.id == id) {
            self.set_current(index)
        } else {
            false
        }
    }

    /// Move to next track
    // Method name `next` is standard for playback control, but does not implement `Iterator`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&QueueItem> {
        let next_index = if let Some(ref order) = self.shuffle_order {
            // Shuffle mode
            if self.shuffle_position + 1 < order.len() {
                self.shuffle_position += 1;
                Some(order[self.shuffle_position])
            } else {
                None
            }
        } else {
            // Normal mode
            self.current_index
                .map(|i| i + 1)
                .filter(|&i| i < self.items.len())
        };

        if let Some(index) = next_index {
            self.set_current(index);
            self.current()
        } else {
            None
        }
    }

    /// Move to previous track
    pub fn previous(&mut self) -> Option<&QueueItem> {
        // Check history first
        if let Some(id) = self.history.pop_back() {
            if let Some(index) = self.items.iter().position(|item| item.id == id) {
                self.current_index = Some(index);
                return self.current();
            }
        }

        // Fall back to previous in order
        let prev_index = if let Some(ref order) = self.shuffle_order {
            if self.shuffle_position > 0 {
                self.shuffle_position -= 1;
                Some(order[self.shuffle_position])
            } else {
                None
            }
        } else {
            self.current_index.and_then(|i| i.checked_sub(1))
        };

        if let Some(index) = prev_index {
            self.current_index = Some(index);
            self.current()
        } else {
            None
        }
    }

    /// Enable shuffle mode
    pub fn shuffle(&mut self) {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();

        let mut order: Vec<usize> = (0..self.items.len()).collect();

        // Keep current track at current position if there is one
        if let Some(current) = self.current_index {
            order.retain(|&i| i != current);
            order.shuffle(&mut rng);
            order.insert(0, current);
            self.shuffle_position = 0;
        } else {
            order.shuffle(&mut rng);
        }

        self.shuffle_order = Some(order);
    }

    /// Disable shuffle mode
    pub fn unshuffle(&mut self) {
        self.shuffle_order = None;
    }

    /// Check if shuffle is enabled
    #[must_use]
    pub fn is_shuffled(&self) -> bool {
        self.shuffle_order.is_some()
    }

    /// Get queue length
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if queue is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get all items
    #[must_use]
    pub fn items(&self) -> &[QueueItem] {
        &self.items
    }

    /// Get item by index
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&QueueItem> {
        self.items.get(index)
    }

    /// Get item by ID
    #[must_use]
    pub fn get_by_id(&self, id: QueueItemId) -> Option<&QueueItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Add to history
    fn add_to_history(&mut self, id: QueueItemId) {
        self.history.push_back(id);
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    /// Get upcoming tracks
    #[must_use]
    pub fn upcoming(&self, count: usize) -> Vec<&QueueItem> {
        let start = self.current_index.map_or(0, |i| i + 1);

        if let Some(ref order) = self.shuffle_order {
            order[self.shuffle_position + 1..]
                .iter()
                .take(count)
                .filter_map(|&i| self.items.get(i))
                .collect()
        } else {
            self.items[start..].iter().take(count).collect()
        }
    }
}

impl Default for PlaybackQueue {
    fn default() -> Self {
        Self::new()
    }
}
