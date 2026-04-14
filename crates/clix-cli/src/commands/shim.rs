use anyhow::Result;
use clix_core::state::home_dir;
use crate::output::print_json;

pub fn list(json: bool) -> Result<()> {
    let home = home_dir();
    let bin_dir = home.join("bin");
    if !bin_dir.exists() {
        if json {
            print_json(&serde_json::json!({"shims": []}));
        } else {
            println!("no shims installed (run `clix init --install-shims <cmd>`)")
        }
        return Ok(());
    }
    let shims: Vec<String> = std::fs::read_dir(&bin_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    if json {
        print_json(&serde_json::json!({"shims": shims, "bin_dir": bin_dir}));
    } else {
        if shims.is_empty() {
            println!("no shims installed");
        } else {
            for s in &shims {
                println!("{}", bin_dir.join(s).display());
            }
        }
    }
    Ok(())
}

pub fn uninstall(command: &str) -> Result<()> {
    let home = home_dir();
    let target = home.join("bin").join(command);
    if !target.exists() {
        anyhow::bail!("shim not found: {}", target.display());
    }
    std::fs::remove_file(&target)?;
    println!("removed shim: {}", target.display());
    Ok(())
}
