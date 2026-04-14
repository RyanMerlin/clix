use crate::error::Result;
use crate::manifest::loader::load_dir;
use crate::policy::PolicyBundle;
use crate::registry::{CapabilityRegistry, WorkflowRegistry};
use crate::state::ClixState;

pub fn build_registry(state: &ClixState) -> Result<CapabilityRegistry> {
    let mut all_caps: Vec<crate::manifest::capability::CapabilityManifest> = load_dir(&state.capabilities_dir)?;
    if state.packs_dir.exists() {
        for entry in std::fs::read_dir(&state.packs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let mut pack_caps = load_dir(&entry.path().join("capabilities"))?;
                all_caps.append(&mut pack_caps);
            }
        }
    }
    let caps = if state.config.active_profiles.is_empty() {
        all_caps
    } else {
        let active = load_active_profiles(state)?;
        let allowed: std::collections::HashSet<String> = active.iter().flat_map(|p| p.capabilities.iter().cloned()).collect();
        if allowed.is_empty() { all_caps } else { all_caps.into_iter().filter(|c| allowed.contains(&c.name)).collect() }
    };
    Ok(CapabilityRegistry::from_vec(caps))
}

pub fn build_workflow_registry(state: &ClixState) -> Result<WorkflowRegistry> {
    let mut all = load_dir(&state.workflows_dir)?;
    if state.packs_dir.exists() {
        for entry in std::fs::read_dir(&state.packs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let mut wfs = load_dir(&entry.path().join("workflows"))?;
                all.append(&mut wfs);
            }
        }
    }
    Ok(WorkflowRegistry::from_vec(all))
}

pub fn load_policy(state: &ClixState) -> Result<PolicyBundle> {
    if state.policy_path.exists() {
        let content = std::fs::read_to_string(&state.policy_path)?;
        Ok(serde_yaml::from_str(&content)?)
    } else {
        Ok(PolicyBundle::default())
    }
}

fn load_active_profiles(state: &ClixState) -> Result<Vec<crate::manifest::profile::ProfileManifest>> {
    let mut all: Vec<crate::manifest::profile::ProfileManifest> = load_dir(&state.profiles_dir)?;
    if state.packs_dir.exists() {
        for entry in std::fs::read_dir(&state.packs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let mut pps = load_dir(&entry.path().join("profiles"))?;
                all.append(&mut pps);
            }
        }
    }
    Ok(all.into_iter().filter(|p| state.config.active_profiles.contains(&p.name)).collect())
}
