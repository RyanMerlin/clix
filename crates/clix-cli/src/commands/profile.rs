use anyhow::Result;
use clix_core::manifest::loader::load_dir;
use clix_core::manifest::profile::ProfileManifest;
use clix_core::state::{home_dir, ClixState};
use crate::output::print_json;

pub fn list(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    // Load global profiles + pack-bundled profiles
    let mut profiles: Vec<ProfileManifest> = load_dir(&state.profiles_dir).unwrap_or_default();
    if state.packs_dir.exists() {
        for entry in std::fs::read_dir(&state.packs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let mut pp = load_dir::<ProfileManifest>(&entry.path().join("profiles")).unwrap_or_default();
                profiles.append(&mut pp);
            }
        }
    }
    if json { print_json(&profiles); }
    else {
        for p in &profiles {
            let active = if state.config.active_profiles.contains(&p.name) { "*" } else { " " };
            println!("{active} {}", p.name);
        }
    }
    Ok(())
}

pub fn activate(name: &str) -> Result<()> {
    let mut state = ClixState::load(home_dir())?;
    if !state.config.active_profiles.contains(&name.to_string()) {
        state.config.active_profiles.push(name.to_string());
        save_config(&state)?;
        println!("activated: {name}");
    } else {
        println!("{name} already active");
    }
    Ok(())
}

pub fn deactivate(name: &str) -> Result<()> {
    let mut state = ClixState::load(home_dir())?;
    state.config.active_profiles.retain(|p| p != name);
    save_config(&state)?;
    println!("deactivated: {name}");
    Ok(())
}

fn save_config(state: &ClixState) -> Result<()> {
    let yaml = serde_yaml::to_string(&state.config)?;
    std::fs::write(&state.config_path, yaml)?;
    Ok(())
}
