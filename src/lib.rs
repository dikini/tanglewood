//! Shared, headless Tanglewood application and command contracts.
pub mod app;
pub mod commands;
pub mod notes;
pub use app::App;

pub mod store;

pub mod extensions;
pub mod markdown;

#[cfg(feature = "gui")]
pub mod ui;

#[cfg(all(target_os = "android", feature = "gui"))]
mod android;

pub mod files;

#[cfg(all(target_os = "linux", feature = "mcp"))]
pub mod mcp;

#[cfg(feature = "sync")]
pub mod sync;
