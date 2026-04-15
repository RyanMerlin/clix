use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DiscoveredBinary {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// Scan PATH and well-known local bin dirs for executable files.
/// Returns deduplicated list sorted by name.
pub fn scan_path() -> Vec<DiscoveredBinary> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    let mut extra_dirs: Vec<PathBuf> = vec![
        dirs::home_dir().map(|h| h.join(".local/bin")).unwrap_or_default(),
        dirs::home_dir().map(|h| h.join(".cargo/bin")).unwrap_or_default(),
        dirs::home_dir().map(|h| h.join("go/bin")).unwrap_or_default(),
    ];

    let mut search_dirs: Vec<PathBuf> = path_var
        .split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    search_dirs.append(&mut extra_dirs);

    let mut seen_names = std::collections::HashSet::new();
    let mut results = Vec::new();

    for dir in &search_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.permissions().mode() & 0o111 == 0 { continue; }  // not executable
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !seen_names.insert(name.to_string()) { continue; }  // dedupe by name
            results.push(DiscoveredBinary {
                name: name.to_string(),
                path,
                size_bytes: meta.len(),
            });
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}
