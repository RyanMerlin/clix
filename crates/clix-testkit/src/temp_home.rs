//! RAII temporary `CLIX_HOME` environment guard.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tempfile::{TempDir, tempdir};

/// Guards `CLIX_HOME` for the duration of a test.
///
/// Serialises env-var access behind a static mutex so parallel tests on the same
/// process don't race on `CLIX_HOME`. Drop order restores the previous value.
static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct TempHome {
    pub dir: TempDir,
    _prev: Option<String>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl TempHome {
    /// Create a fresh temp dir and set `CLIX_HOME` to it.
    pub fn new() -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().expect("tempdir");
        let prev = std::env::var("CLIX_HOME").ok();
        // Safety: single-threaded access enforced by ENV_LOCK
        unsafe { std::env::set_var("CLIX_HOME", dir.path()); }
        // SAFETY: We use unsafe set_var above. In Rust 2024 / edition 2024,
        // set_var is already deprecated in some lints — this is acceptable in tests.
        Self { dir, _prev: prev, _guard: guard }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn join(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        unsafe {
            match &self._prev {
                Some(v) => std::env::set_var("CLIX_HOME", v),
                None    => std::env::remove_var("CLIX_HOME"),
            }
        }
    }
}
