use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DiscoveredBinary {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// Extensions that are never real Linux CLI tools — skip them entirely.
/// Catches Windows DLLs, images, data files that appear on PATH in WSL.
static SKIP_EXTENSIONS: &[&str] = &[
    "dll", "DLL", "png", "ico", "bmp", "jpg", "jpeg",
    "json", "xml", "ini", "cfg", "config", "log",
    "txt", "md", "pdf", "zip", "tar", "gz",
    "sys", "mui", "cat", "mum", "manifest",
    "pri", "ptxml", "dsc",
];

/// Returns true if this directory should be excluded from the scan.
/// Skips Windows filesystem mounts (e.g. /mnt/c/) which WSL adds to PATH.
fn is_excluded_dir(dir: &Path) -> bool {
    let s = dir.to_string_lossy();
    // Skip Windows drive mounts (/mnt/c, /mnt/d, ...)
    if s.starts_with("/mnt/") {
        // Allow /mnt/wsl and other non-drive mounts — only skip single-letter drive mounts
        let after_mnt = &s[5..];
        if let Some(rest) = after_mnt.splitn(2, '/').next() {
            if rest.len() == 1 && rest.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
                return true;  // /mnt/c/, /mnt/d/, etc.
            }
        }
    }
    false
}

/// Returns true if the filename has a skippable extension.
fn has_skip_extension(name: &str) -> bool {
    if let Some(ext) = name.rsplit('.').next() {
        if ext != name {  // has an extension
            return SKIP_EXTENSIONS.contains(&ext);
        }
    }
    false
}

/// Scan PATH and well-known local bin dirs for executable files.
/// Excludes Windows drive mounts (WSL /mnt/c etc.) and non-CLI file types.
/// Returns deduplicated list sorted by name.
pub fn scan_path() -> Vec<DiscoveredBinary> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    let extra_dirs: Vec<PathBuf> = [
        dirs::home_dir().map(|h| h.join(".local/bin")),
        dirs::home_dir().map(|h| h.join(".cargo/bin")),
        dirs::home_dir().map(|h| h.join("go/bin")),
    ].into_iter().flatten().collect();

    let mut search_dirs: Vec<PathBuf> = path_var
        .split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .chain(extra_dirs)
        .filter(|d| !is_excluded_dir(d))
        .collect();

    // Dedupe dirs
    search_dirs.dedup();

    let mut seen_names = std::collections::HashSet::new();
    let mut results = Vec::new();

    for dir in &search_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.permissions().mode() & 0o111 == 0 { continue; }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if has_skip_extension(name) { continue; }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_windows_mounts() {
        assert!(is_excluded_dir(Path::new("/mnt/c")));
        assert!(is_excluded_dir(Path::new("/mnt/d/foo")));
        assert!(!is_excluded_dir(Path::new("/mnt/wsl")));
        assert!(!is_excluded_dir(Path::new("/usr/bin")));
    }

    #[test]
    fn test_skip_extensions() {
        assert!(has_skip_extension("foo.dll"));
        assert!(has_skip_extension("KBDOGHAM.DLL"));
        assert!(has_skip_extension("config.json"));
        assert!(!has_skip_extension("git"));
        assert!(!has_skip_extension("kubectl"));
        assert!(!has_skip_extension("node"));
    }
}
