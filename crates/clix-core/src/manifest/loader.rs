use std::path::Path;
use crate::error::{ClixError, Result};

pub fn load_manifest<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = std::fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "json" => Ok(serde_json::from_str(&content)?),
        "yaml" | "yml" => Ok(serde_yaml::from_str(&content)?),
        _ => Err(ClixError::Pack(format!("unsupported extension: {ext}"))),
    }
}

pub fn load_dir<T>(dir: &Path) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut results = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() { continue; }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if matches!(ext, "yaml" | "yml" | "json") {
            match load_manifest::<T>(&path) {
                Ok(m) => results.push(m),
                Err(e) => eprintln!("warn: skipping {}: {e}", path.display()),
            }
        }
    }
    Ok(results)
}
