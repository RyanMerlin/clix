pub mod error;

/// Set to true by the TUI before entering alternate-screen mode.
/// The manifest loader checks this to suppress stderr warnings that would
/// corrupt the TUI display.
pub static TUI_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub mod discovery;
pub mod execution;
pub mod loader;
pub mod manifest;
pub mod packs;
pub mod policy;
pub mod receipts;
pub mod registry;
pub mod sandbox;
pub mod schema;
pub mod secrets;
pub mod state;
pub mod storage;
pub mod template;
