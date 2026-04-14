use anyhow::Result;
use clix_core::state::{ClixConfig, ClixState, home_dir};
use clix_core::packs::seed::seed_builtin_packs;

pub fn run() -> Result<()> {
    let home = home_dir();
    let state = ClixState::from_home(home.clone());
    state.ensure_dirs()?;

    // Load or create config
    let mut config = if state.config_path.exists() {
        let text = std::fs::read_to_string(&state.config_path)?;
        serde_yaml::from_str::<ClixConfig>(&text)?   // propagate parse errors, don't swallow
    } else {
        let config = ClixConfig::default();
        println!("Created {}", state.config_path.display());
        config
    };

    // Seed built-in packs
    let packs_src = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("packs"))),
        Some(std::path::PathBuf::from("packs")),
    ].into_iter().flatten().find(|p| p.exists());
    if let Some(src) = packs_src {
        seed_builtin_packs(&state.packs_dir, &src)?;
        println!("Seeded built-in packs");
    }

    // Auto-activate base profile if nothing is active
    if config.active_profiles.is_empty() {
        config.active_profiles.push("base".to_string());
        println!("Activated default profile: base");
    }

    // Write config (single write)
    let yaml = serde_yaml::to_string(&config)?;
    std::fs::write(&state.config_path, yaml)?;

    println!("clix initialized at {}", home.display());
    Ok(())
}
