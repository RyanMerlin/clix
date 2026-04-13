use std::path::Path;
use serde::Serialize;
use crate::error::Result;
use super::discover::discover_pack;

#[derive(Debug, Serialize)]
pub struct DiffReport {
    pub pack_name: String,
    pub version_change: Option<(u32, u32)>,
    pub capabilities_added: Vec<String>,
    pub capabilities_removed: Vec<String>,
    pub capabilities_changed: Vec<String>,
    pub profiles_added: Vec<String>,
    pub profiles_removed: Vec<String>,
    pub workflows_added: Vec<String>,
    pub workflows_removed: Vec<String>,
}

/// Compare an installed pack (by its installed directory path) with a new pack source.
pub fn diff_pack(installed: &Path, new_src: &Path) -> Result<DiffReport> {
    let old = discover_pack(installed)?;
    let new = discover_pack(new_src)?;

    let old_caps: std::collections::HashSet<_> = old.capabilities.iter().map(|c| c.name.clone()).collect();
    let new_caps: std::collections::HashSet<_> = new.capabilities.iter().map(|c| c.name.clone()).collect();

    let old_profiles: std::collections::HashSet<_> = old.profiles.iter().map(|p| p.name.clone()).collect();
    let new_profiles: std::collections::HashSet<_> = new.profiles.iter().map(|p| p.name.clone()).collect();

    let old_wf: std::collections::HashSet<_> = old.workflows.iter().map(|w| w.name.clone()).collect();
    let new_wf: std::collections::HashSet<_> = new.workflows.iter().map(|w| w.name.clone()).collect();

    let changed: Vec<String> = old.capabilities.iter()
        .filter_map(|old_cap| {
            new.capabilities.iter().find(|nc| nc.name == old_cap.name && nc.version != old_cap.version)
                .map(|_| old_cap.name.clone())
        })
        .collect();

    Ok(DiffReport {
        pack_name: old.pack.name.clone(),
        version_change: if old.pack.version != new.pack.version {
            Some((old.pack.version, new.pack.version))
        } else {
            None
        },
        capabilities_added:   new_caps.difference(&old_caps).cloned().collect(),
        capabilities_removed: old_caps.difference(&new_caps).cloned().collect(),
        capabilities_changed: changed,
        profiles_added:       new_profiles.difference(&old_profiles).cloned().collect(),
        profiles_removed:     old_profiles.difference(&new_profiles).cloned().collect(),
        workflows_added:      new_wf.difference(&old_wf).cloned().collect(),
        workflows_removed:    old_wf.difference(&new_wf).cloned().collect(),
    })
}
