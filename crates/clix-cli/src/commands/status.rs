use anyhow::Result;
use clix_core::sandbox::sandbox_enforced;
use clix_core::state::{home_dir, ClixState};
use crate::output::{print_json, print_kv};

pub fn run(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let enforced = sandbox_enforced();
    if json {
        print_json(&serde_json::json!({
            "home": state.home,
            "configPath": state.config_path,
            "activeProfiles": state.config.active_profiles,
            "defaultEnv": state.config.default_env,
            "approvalMode": format!("{:?}", state.config.approval_mode),
            "sandboxEnforced": enforced,
        }));
    } else {
        print_kv(&[
            ("home",            state.home.display().to_string()),
            ("config",          state.config_path.display().to_string()),
            ("active profiles", state.config.active_profiles.join(", ")),
            ("default env",     state.config.default_env.clone()),
            ("approval mode",   format!("{:?}", state.config.approval_mode)),
            ("sandbox",         if enforced { "enforced (Landlock)" } else { "not enforced" }.to_string()),
        ]);
    }
    Ok(())
}
