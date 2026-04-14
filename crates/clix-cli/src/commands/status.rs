use anyhow::Result;
use clix_core::sandbox::sandbox_enforced;
use clix_core::state::{home_dir, ClixState};
use clix_core::loader::build_registry;
use clix_core::manifest::loader::load_dir;
use clix_core::manifest::pack::PackManifest;
use crate::output::{print_json, print_kv};

pub fn run(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let enforced = sandbox_enforced();

    // Load packs and registry for counts
    let packs: Vec<PackManifest> = load_dir(&state.packs_dir).unwrap_or_default();
    let registry = build_registry(&state).unwrap_or_default();

    if json {
        print_json(&serde_json::json!({
            "home": state.home,
            "configPath": state.config_path,
            "activeProfiles": state.config.active_profiles,
            "defaultEnv": state.config.default_env,
            "approvalMode": format!("{:?}", state.config.approval_mode),
            "sandboxEnforced": enforced,
            "packCount": packs.len(),
            "capabilityCount": registry.all().len(),
        }));
    } else {
        print_kv(&[
            ("home",            state.home.display().to_string()),
            ("config",          state.config_path.display().to_string()),
            ("active profiles", state.config.active_profiles.len().to_string()),
            ("packs",           packs.len().to_string()),
            ("capabilities",    registry.all().len().to_string()),
            ("default env",     state.config.default_env.clone()),
            ("approval mode",   format!("{:?}", state.config.approval_mode)),
            ("sandbox",         if enforced { "enforced (Landlock)" } else { "not enforced" }.to_string()),
        ]);
    }
    Ok(())
}
