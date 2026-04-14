use anyhow::Result;
use clix_core::sandbox::sandbox_enforced;
use clix_core::state::{home_dir, ClixState};
use clix_core::loader::build_registry;
use clix_core::manifest::pack::PackManifest;
use crate::output::{print_json, print_kv};

pub fn run(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let enforced = sandbox_enforced();

    // Count installed packs by walking subdirectories (each pack lives at packs_dir/<name>/pack.yaml)
    let pack_count = if state.packs_dir.exists() {
        std::fs::read_dir(&state.packs_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .filter(|e| e.path().join("pack.yaml").exists())
                    .filter_map(|e| {
                        std::fs::read_to_string(e.path().join("pack.yaml")).ok()
                    })
                    .filter_map(|s| serde_yaml::from_str::<PackManifest>(&s).ok())
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };
    let registry = build_registry(&state).unwrap_or_default();

    if json {
        print_json(&serde_json::json!({
            "home": state.home,
            "configPath": state.config_path,
            "activeProfiles": state.config.active_profiles,
            "defaultEnv": state.config.default_env,
            "approvalMode": format!("{:?}", state.config.approval_mode),
            "sandboxEnforced": enforced,
            "packCount": pack_count,
            "capabilityCount": registry.all().len(),
        }));
    } else {
        print_kv(&[
            ("home",            state.home.display().to_string()),
            ("config",          state.config_path.display().to_string()),
            ("active profiles", state.config.active_profiles.join(", ")),
            ("packs",           pack_count.to_string()),
            ("capabilities",    registry.all().len().to_string()),
            ("default env",     state.config.default_env.clone()),
            ("approval mode",   format!("{:?}", state.config.approval_mode)),
            ("sandbox",         if enforced { "enforced (Landlock)" } else { "not enforced" }.to_string()),
        ]);
    }
    Ok(())
}
