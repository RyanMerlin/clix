//! Golden help-output fixture loader.
//!
//! Fixtures live under `crates/clix-core/tests/fixtures/help/<tool>-<version>.txt`.
//! Use `load_help` to read a fixture from that directory.

use std::path::PathBuf;

/// Returns the path to the fixtures/help directory for clix-core tests.
/// Works from any cargo test invocation because `CARGO_MANIFEST_DIR` points at the
/// crate being tested; this crate is in `crates/clix-testkit/`, so we walk up two levels.
fn fixtures_dir() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    root.join("crates/clix-core/tests/fixtures/help")
}

/// Read the golden help text fixture for the given tool and version string
/// (e.g. `load_help("gcloud", "latest")`).
/// Panics if the fixture does not exist (intentional — missing fixture = test gap).
pub fn load_help(tool: &str, version: &str) -> String {
    let path = fixtures_dir().join(format!("{tool}-{version}.txt"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()))
}

/// Returns all `(tool, version)` pairs for available fixtures.
pub fn available_help_fixtures() -> Vec<(String, String)> {
    let dir = fixtures_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![]; };
    entries
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".txt")?;
            let (tool, version) = stem.rsplit_once('-')?;
            Some((tool.to_string(), version.to_string()))
        })
        .collect()
}
