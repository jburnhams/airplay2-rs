//! DAAP/DMAP metadata protocol for RAOP

mod artwork;
mod dmap;
mod metadata;
mod progress;

#[cfg(test)]
mod tests;

pub use artwork::{Artwork, ArtworkFormat};
pub use dmap::{DmapEncoder, DmapTag};
pub use metadata::{MetadataBuilder, TrackMetadata};
pub use progress::PlaybackProgress;
