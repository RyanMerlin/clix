use crate::error::Result;
use crate::manifest::loader::load_dir;
use crate::manifest::profile::{ProfileFolderBinding, ProfileManifest, ProfileSecretBinding};
use crate::policy::PolicyBundle;
use crate::registry::{CapabilityRegistry, WorkflowRegistry};
use crate::state::ClixState;

pub fn build_registry(state: &ClixState) -> Result<CapabilityRegistry> {
    let mut all_caps: Vec<crate::manifest::capability::CapabilityManifest> =
        load_dir(&state.capabilities_dir)?;
    if state.storage.exists(&state.packs_dir) {
        for path in state.storage.list(&state.packs_dir)? {
            if state.storage.is_dir(&path) {
                let mut pack_caps = load_dir(&path.join("capabilities"))?;
                all_caps.append(&mut pack_caps);
            }
        }
    }
    let caps = if state.config.active_profiles.is_empty() {
        all_caps
    } else {
        let active = load_active_profile_manifests(state)?;
        if active.is_empty() {
            return Err(crate::error::ClixError::Config(format!(
                "active profile(s) not found: {}",
                state.config.active_profiles.join(", ")
            )));
        }
        let allowed: std::collections::HashSet<String> = active
            .iter()
            .flat_map(|p| p.capabilities.iter().cloned())
            .collect();
        all_caps
            .into_iter()
            .filter(|c| allowed.contains(&c.name))
            .collect()
    };
    Ok(CapabilityRegistry::from_vec(caps))
}

pub fn build_workflow_registry(state: &ClixState) -> Result<WorkflowRegistry> {
    let mut all = load_dir(&state.workflows_dir)?;
    if state.storage.exists(&state.packs_dir) {
        for path in state.storage.list(&state.packs_dir)? {
            if state.storage.is_dir(&path) {
                let mut wfs = load_dir(&path.join("workflows"))?;
                all.append(&mut wfs);
            }
        }
    }
    Ok(WorkflowRegistry::from_vec(all))
}

pub fn load_policy(state: &ClixState) -> Result<PolicyBundle> {
    if state.storage.exists(&state.policy_path) {
        let content = state.storage.read_to_string(&state.policy_path)?;
        Ok(serde_yaml::from_str(&content)?)
    } else {
        Ok(PolicyBundle::default())
    }
}

pub fn load_active_profile_manifests(state: &ClixState) -> Result<Vec<ProfileManifest>> {
    use std::collections::HashMap;
    let mut by_name: HashMap<String, ProfileManifest> = HashMap::new();
    for p in load_dir::<ProfileManifest>(&state.profiles_dir)? {
        by_name.insert(p.name.clone(), p);
    }
    if state.storage.exists(&state.packs_dir) {
        for path in state.storage.list(&state.packs_dir)? {
            if state.storage.is_dir(&path) {
                for p in load_dir::<ProfileManifest>(&path.join("profiles"))? {
                    by_name.entry(p.name.clone()).or_insert(p);
                }
            }
        }
    }
    let mut missing = Vec::new();
    let mut active = Vec::new();
    for name in &state.config.active_profiles {
        match by_name.remove(name) {
            Some(profile) => active.push(profile),
            None => missing.push(name.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(crate::error::ClixError::Config(format!(
            "active profile(s) not found: {}",
            missing.join(", ")
        )));
    }
    Ok(active)
}

pub fn active_profile_bindings(
    state: &ClixState,
) -> Result<(Vec<ProfileSecretBinding>, Vec<ProfileFolderBinding>)> {
    if state.config.active_profiles.is_empty() {
        return Ok((vec![], vec![]));
    }

    let profiles = load_active_profile_manifests(state)?;
    if profiles.is_empty() {
        return Err(crate::error::ClixError::Config(format!(
            "active profile(s) not found: {}",
            state.config.active_profiles.join(", ")
        )));
    }

    let mut secret_bindings = Vec::new();
    let mut folder_bindings = Vec::new();
    for profile in profiles {
        secret_bindings.extend(profile.secret_bindings);
        folder_bindings.extend(profile.folder_bindings);
    }

    Ok((secret_bindings, folder_bindings))
}
