//! clix-testkit — shared test infrastructure for the clix workspace.
//!
//! Provides:
//! - [`TempHome`] — temp dir + RAII `CLIX_HOME` env guard.
//! - [`capability`] — fluent `CapabilityManifest` / `CapabilityRegistry` builders.
//! - [`receipts`] — in-memory `ReceiptStore` factory.
//! - [`serve`] — `ServeState` bootstrapper.
//! - [`mock`] — wiremock OAuth2 server and Unix-socket broker echo server.
//! - [`fixtures`] — golden help-output loader.

pub mod capability;
pub mod fixtures;
pub mod mock;
pub mod receipts;
pub mod serve;
pub mod temp_home;

pub use temp_home::TempHome;

// ─── Convenience re-exports ───────────────────────────────────────────────────

pub use clix_core::manifest::capability::{
    Backend, CapabilityManifest, IsolationTier, RiskLevel, SideEffectClass,
};
pub use clix_core::policy::{PolicyAction, PolicyBundle, PolicyRule};
pub use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
pub use clix_core::receipts::ReceiptStore;
pub use clix_core::state::ClixState;
pub use clix_serve::dispatch::ServeState;
