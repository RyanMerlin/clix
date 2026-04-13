use anyhow::Result;
use clix_core::loader::build_registry;
use clix_core::state::{home_dir, ClixState};
use crate::output::{print_json, print_kv};

pub fn list(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let registry = build_registry(&state)?;
    let caps: Vec<_> = registry.all().into_iter().collect();
    if json {
        print_json(&caps);
    } else {
        for cap in &caps {
            println!("{:<40} {}", cap.name, cap.description.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

pub fn show(name: &str, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let registry = build_registry(&state)?;
    match registry.get(name) {
        Some(cap) if json => print_json(cap),
        Some(cap) => print_kv(&[
            ("name",         cap.name.clone()),
            ("version",      cap.version.to_string()),
            ("description",  cap.description.clone().unwrap_or_default()),
            ("risk",         format!("{:?}", cap.risk)),
            ("side effects", format!("{:?}", cap.side_effect_class)),
        ]),
        None => anyhow::bail!("capability not found: {name}"),
    }
    Ok(())
}
