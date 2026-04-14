use std::path::Path;
use serde::Serialize;
use crate::error::Result;
use crate::manifest::loader::load_manifest;
use crate::manifest::pack::PackManifest;
use crate::manifest::capability::CapabilityManifest;
use crate::manifest::profile::ProfileManifest;
use crate::manifest::workflow::WorkflowManifest;
use crate::manifest::loader::load_dir;

#[derive(Debug, Serialize)]
pub struct DiscoverReport {
    pub pack: PackManifest,
    pub profiles: Vec<ProfileManifest>,
    pub capabilities: Vec<CapabilityManifest>,
    pub workflows: Vec<WorkflowManifest>,
    pub warnings: Vec<String>,
}

/// Inspect a pack directory without installing it.
pub fn discover_pack(path: &Path) -> Result<DiscoverReport> {
    let mut warnings = vec![];

    let manifest_path = ["pack.yaml", "pack.yml", "pack.json"]
        .iter()
        .map(|f| path.join(f))
        .find(|p| p.exists())
        .ok_or_else(|| crate::error::ClixError::Pack(
            format!("no pack.yaml found in {}", path.display())
        ))?;

    let pack: PackManifest = load_manifest(&manifest_path)?;

    let profiles: Vec<ProfileManifest> = load_dir(&path.join("profiles"))
        .unwrap_or_else(|e| { warnings.push(format!("profiles: {e}")); vec![] });
    let capabilities: Vec<CapabilityManifest> = load_dir(&path.join("capabilities"))
        .unwrap_or_else(|e| { warnings.push(format!("capabilities: {e}")); vec![] });
    let workflows: Vec<WorkflowManifest> = load_dir(&path.join("workflows"))
        .unwrap_or_else(|e| { warnings.push(format!("workflows: {e}")); vec![] });

    Ok(DiscoverReport { pack, profiles, capabilities, workflows, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_discover_minimal_pack() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pack.yaml"), "name: my-pack\nversion: 1\n").unwrap();
        let report = discover_pack(dir.path()).unwrap();
        assert_eq!(report.pack.name, "my-pack");
        assert!(report.capabilities.is_empty());
    }
}
