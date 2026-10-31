//! fm-editor — real-time memory editor for Football Manager 26 and 24.
//!
//! This library crate holds the shared code for the main editor binary
//! (`src/main.rs`) and the standalone dump/scan tools (`src/bin/*.rs`).
//! Everything that needs version-aware schemas, process attachment, or
//! field lookups lives here so it has exactly one implementation.

pub mod affiliation;
pub mod app;
pub mod best_xi;
pub mod bridge;
pub mod cache;
pub mod club;
pub mod fields;
pub mod game_version;
pub mod player;
pub mod process;
pub mod ratings;
pub mod settings;
pub mod staff;
pub mod ui;
pub mod undo;
pub mod write_tracker;

#[cfg(debug_assertions)]
pub mod debug;
