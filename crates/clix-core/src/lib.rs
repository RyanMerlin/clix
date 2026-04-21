pub mod error;

/// Set to true by the TUI before entering alternate-screen mode.
/// The manifest loader checks this to suppress stderr warnings that would
/// corrupt the TUI display.
pub static TUI_MODE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
pub mod manifest;
pub mod policy;
pub mod schema;
pub mod state;
pub mod template;
pub mod registry;
pub mod secrets;
pub mod execution;
pub mod receipts;
pub mod sandbox;
pub mod packs;
pub mod loader;
pub mod discovery;
pub mod storage;
