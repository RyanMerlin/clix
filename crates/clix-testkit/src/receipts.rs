//! In-memory `ReceiptStore` factory.

use clix_core::receipts::ReceiptStore;

/// Open a hermetic in-memory SQLite-backed receipt store.
pub fn memory_store() -> ReceiptStore {
    ReceiptStore::open(std::path::Path::new(":memory:")).expect("in-memory receipt store")
}
