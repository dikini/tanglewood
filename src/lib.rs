//! Shared, headless Tanglewood application and command contracts.
pub mod app;
pub mod commands;
pub mod notes;
pub use app::App;

pub mod store;

pub mod document_world;
pub mod extensions;
#[cfg(feature = "documents")]
pub mod illustration;
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

#[cfg(all(target_os = "linux", feature = "conversation"))]
pub mod conversation;

#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
pub(crate) mod conversation_runtime;

#[cfg(all(target_os = "linux", any(feature = "codex", feature = "kimi")))]
pub(crate) mod model_catalog;

#[cfg(all(target_os = "linux", feature = "codex"))]
pub mod codex;

#[cfg(all(target_os = "linux", feature = "kimi"))]
pub mod kimi;

#[cfg(feature = "lan")]
pub mod lan;

#[cfg(feature = "lan")]
mod lan_owner;

#[cfg(all(target_os = "android", feature = "gui"))]
pub mod android_picker;

#[cfg(all(feature = "gui", any(target_os = "android", test)))]
pub mod android_input;

#[cfg(feature = "documents")]
pub mod document_jobs;
#[cfg(feature = "documents")]
pub mod pandoc;

pub mod research;
pub mod research_assets;

pub(crate) mod research_import;

pub mod research_desktop;
pub mod research_reader;

#[cfg(feature = "wasm-extensions")]
pub mod wasm_extensions;

pub mod plugin_permissions;
pub mod research_authority;
#[cfg(feature = "wasm-extensions")]
pub mod plugin_manifest;
#[cfg(feature = "wasm-extensions")]
pub mod plugin_registry;
#[cfg(feature = "wasm-extensions")]
pub mod wasm_commands;

#[cfg(feature = "wasm-extensions")]
pub mod plugin_jobs;
#[cfg(any(feature = "wasm-extensions", feature = "mcp"))]
pub(crate) mod command_receipts;

#[cfg(feature = "wasm-extensions")]
pub mod plugin_io;
