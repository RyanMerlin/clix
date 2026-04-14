use anyhow::Result;
use std::path::Path;
use clix_core::manifest::pack::PackManifest;
use clix_core::packs::{
    bundle_pack, diff_pack, discover_pack, install_pack, onboard_cli, publish_pack, scaffold_pack,
    validate_pack,
};
use clix_core::packs::scaffold::Preset;
use clix_core::state::{home_dir, ClixState};
use crate::output::print_json;

pub fn list(json: bool, available: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;

    if available {
        // List available packs (in source but not installed)
        let packs_src = [
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("packs"))),
            Some(std::path::PathBuf::from("packs")),
        ].into_iter().flatten().find(|p| p.exists());

        if let Some(src) = packs_src {
            let mut packs = vec![];
            for entry in std::fs::read_dir(&src)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    // Check if NOT installed
                    if !state.packs_dir.join(&*name_str).exists() {
                        let pack_file = entry.path().join("pack.yaml");
                        if pack_file.exists() {
                            let content = std::fs::read_to_string(&pack_file)?;
                            if let Ok(p) = serde_yaml::from_str::<PackManifest>(&content) {
                                packs.push(p);
                            }
                        }
                    }
                }
            }
            packs.sort_by(|a, b| a.name.cmp(&b.name));
            if json { print_json(&packs); }
            else {
                for p in &packs {
                    println!("{:<30} v{}  {}", p.name, p.version, p.description.as_deref().unwrap_or(""));
                }
            }
        }
    } else {
        // List installed packs
        if !state.packs_dir.exists() { return Ok(()); }
        let mut packs = vec![];
        for entry in std::fs::read_dir(&state.packs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let pack_file = entry.path().join("pack.yaml");
                if pack_file.exists() {
                    let content = std::fs::read_to_string(&pack_file)?;
                    if let Ok(p) = serde_yaml::from_str::<PackManifest>(&content) {
                        packs.push(p);
                    }
                }
            }
        }
        packs.sort_by(|a, b| a.name.cmp(&b.name));
        if json { print_json(&packs); }
        else {
            for p in &packs {
                println!("{:<30} v{}  {}", p.name, p.version, p.description.as_deref().unwrap_or(""));
            }
        }
    }
    Ok(())
}

pub fn show(name: &str, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let pack_dir = state.packs_dir.join(name);
    anyhow::ensure!(pack_dir.exists(), "pack not found: {name}");
    let report = discover_pack(&pack_dir)?;
    if json { print_json(&report); }
    else {
        println!("name:         {}", report.pack.name);
        println!("version:      {}", report.pack.version);
        println!("capabilities: {}", report.capabilities.len());
        println!("profiles:     {}", report.profiles.len());
        println!("workflows:    {}", report.workflows.len());
    }
    Ok(())
}

pub fn discover(path: &str, json: bool) -> Result<()> {
    let report = discover_pack(Path::new(path))?;
    if json { print_json(&report); }
    else {
        println!("pack:         {}", report.pack.name);
        println!("capabilities: {}", report.capabilities.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
        for w in &report.warnings { eprintln!("warn: {w}"); }
    }
    Ok(())
}

pub fn validate(path: &str) -> Result<()> {
    let errors = validate_pack(Path::new(path))?;
    if errors.is_empty() {
        println!("ok");
        Ok(())
    } else {
        for e in &errors { eprintln!("error: [{}] {}", e.path, e.message); }
        anyhow::bail!("{} validation error(s)", errors.len())
    }
}

pub fn diff(installed_name: &str, new_path: &str, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let installed = state.packs_dir.join(installed_name);
    anyhow::ensure!(installed.exists(), "installed pack not found: {installed_name}");
    let report = diff_pack(&installed, Path::new(new_path))?;
    if json { print_json(&report); }
    else {
        if let Some((old, new)) = report.version_change { println!("version: {old} → {new}"); }
        if !report.capabilities_added.is_empty()   { println!("+ capabilities: {}", report.capabilities_added.join(", ")); }
        if !report.capabilities_removed.is_empty() { println!("- capabilities: {}", report.capabilities_removed.join(", ")); }
        if !report.capabilities_changed.is_empty() { println!("~ capabilities: {}", report.capabilities_changed.join(", ")); }
    }
    Ok(())
}

pub fn install(path: &str) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let dest = install_pack(Path::new(path), &state.packs_dir)?;
    println!("installed: {}", dest.display());
    Ok(())
}

pub fn bundle(path: &str) -> Result<()> {
    let zip = bundle_pack(Path::new(path), Path::new("."))?;
    println!("bundled: {}", zip.display());
    Ok(())
}

pub fn publish(path: &str) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    publish_pack(Path::new(path), &state.bundles_dir)?;
    println!("published");
    Ok(())
}

pub fn scaffold(name: &str, preset_str: &str, command: Option<&str>) -> Result<()> {
    let preset: Preset = preset_str.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let dir = scaffold_pack(name, preset, command, Path::new("."))?;
    println!("scaffolded: {}", dir.display());
    Ok(())
}

pub fn onboard(name: &str, command: &str, json: bool) -> Result<()> {
    let report = onboard_cli(name, command, Path::new("."))?;
    if json { print_json(&report); }
    else {
        println!("cli:       {}", report.cli);
        println!("preset:    {} (confidence {:.0}%)", report.suggested_preset, report.confidence * 100.0);
        println!("subcommands: {}", report.inferred_subcommands.join(", "));
        if let Some(p) = &report.scaffold_path { println!("scaffold:  {}", p.display()); }
        for w in &report.warnings { eprintln!("warn: {w}"); }
    }
    Ok(())
}
