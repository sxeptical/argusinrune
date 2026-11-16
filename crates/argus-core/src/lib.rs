//! Domain types and pure logic for Argus.
//!
//! This crate has no UI and no I/O dependencies beyond serde/serde_json: it is
//! the shared vocabulary the data layer fetches and the Rune app draws. It
//! ports the TypeScript `types/` and the pure (non-network, non-React) parts of
//! `lib/` and `app/hooks/`. See the "argus-core API" section of
//! `RUST_REWRITE.md` for the module map and data-loading approach.

pub mod air_quality;
pub mod amenities;
pub mod amenity_layers;
pub mod api_types;
pub mod bundle_validation;
pub mod data;
pub mod flights;
pub mod format;
pub mod geo;
pub mod geometry;
pub mod layers;
pub mod mall_stores;
pub mod maps_links;
pub mod mobility;
pub mod model;
pub mod mrt_network;
pub mod mrt_routing;
pub mod parks;
pub mod popups;
pub mod preferences;
pub mod search;
pub mod signals;
pub mod types;

pub use model::{Layer, LayerId, enabled_layer_labels};
