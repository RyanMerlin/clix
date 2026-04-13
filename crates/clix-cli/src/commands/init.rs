use anyhow::Result;
use clix_core::state::{ClixConfig, ClixState, home_dir};
use clix_core::packs::seed::seed_builtin_packs;

pub fn run() -> Result<()> {
    let home = home_dir();
    let state = ClixState::from_home(home.clone());
    state.ensure_dirs()?;
    if !state.config_path.exists() {
        let config = ClixConfig::default();
        let yaml = serde_yaml::to_string(&config)?;
        std::fs::write(&state.config_path, yaml)?;
        println!("Created {}", state.config_path.display());
    } else {
        println!("Config already exists: {}", state.config_path.display());
    }
    let packs_src = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("packs"))),
        Some(std::path::PathBuf::from("packs")),
    ].into_iter().flatten().find(|p| p.exists());
    if let Some(src) = packs_src {
        seed_builtin_packs(&state.packs_dir, &src)?;
        println!("Seeded built-in packs");
    }
    println!("clix initialized at {}", home.display());
    Ok(())
}
