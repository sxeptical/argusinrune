//! Domain types and logic for Argus.
//!
//! This crate has no UI and no I/O dependencies: it is the shared vocabulary
//! the data layer fetches and the Rune app draws. Later phases add OSINT
//! entities (events, sources, entities, geo features) here.

pub mod model;

pub use model::{Layer, LayerId};
