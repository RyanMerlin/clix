// Smoke test for profile deduplication — verifies that when the same profile
// name appears in both profiles/ and packs/*/profiles/, only one entry is returned
// and it's the global one (user override wins).
use chrono::Utc;
use clix_core::manifest::profile::{
    IsolationDefaults, ProfileFolderBinding, ProfileManifest, ProfileSecretBinding,
};
use clix_core::state::ClixState;
use std::path::PathBuf;

fn write_profile(dir: &PathBuf, name: &str, description: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let manifest = ProfileManifest {
        name: name.to_string(),
        version: 1,
        description: Some(description.to_string()),
        capabilities: vec![],
        workflows: vec![],
        settings: serde_json::Value::Null,
        isolation_defaults: Default::default(),
        secret_bindings: vec![],
        folder_bindings: vec![],
    };
    let yaml = serde_yaml::to_string(&manifest).unwrap();
    std::fs::write(dir.join(format!("{}.yaml", name)), yaml).unwrap();
}

#[test]
fn global_profile_wins_over_pack_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();

    // Write global profile with "global-description"
    write_profile(&home.join("profiles"), "myprofile", "global-description");

    // Write pack profile with same name but different description
    let pack_profiles = home.join("packs").join("mypck").join("profiles");
    write_profile(&pack_profiles, "myprofile", "pack-description");

    // Also write pack.yaml so the pack is recognized
    std::fs::create_dir_all(home.join("packs").join("mypck")).unwrap();
    std::fs::write(
        home.join("packs").join("mypck").join("pack.yaml"),
        "name: mypck\nversion: 1\ncapabilities: []\n",
    )
    .unwrap();

    // Use ClixState to load profiles the same way the app does
    let state = ClixState::load(home).unwrap();

    // Collect all profiles via the same logic as load_all_profiles in app.rs
    use clix_core::manifest::loader::load_dir;
    use std::collections::HashMap;

    let mut by_name: HashMap<String, ProfileManifest> = HashMap::new();
    for p in load_dir::<ProfileManifest>(&state.profiles_dir).unwrap_or_default() {
        by_name.insert(p.name.clone(), p);
    }
    // Load pack profiles
    if state.packs_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&state.packs_dir)
    {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                for p in
                    load_dir::<ProfileManifest>(&entry.path().join("profiles")).unwrap_or_default()
                {
                    by_name.entry(p.name.clone()).or_insert(p);
                }
            }
        }
    }

    // Only one "myprofile" should exist
    assert_eq!(
        by_name.values().filter(|p| p.name == "myprofile").count(),
        1
    );
    // And it should be the global one
    let profile = by_name.get("myprofile").unwrap();
    assert_eq!(profile.description.as_deref(), Some("global-description"));
}

#[test]
fn missing_active_profile_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let mut state = ClixState::from_home(home);
    state.config.active_profiles = vec!["missing".to_string()];
    let err = clix_core::loader::build_registry(&state).unwrap_err();
    assert!(err.to_string().contains("active profile(s) not found"));
}

#[test]
fn active_profile_bindings_are_collected() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let profile = ProfileManifest {
        name: "readonly".to_string(),
        version: 1,
        description: None,
        capabilities: vec![],
        workflows: vec![],
        settings: serde_json::Value::Null,
        isolation_defaults: IsolationDefaults::default(),
        secret_bindings: vec![ProfileSecretBinding {
            inject_as: "TOKEN".to_string(),
            source: clix_core::manifest::capability::CredentialSource::Literal {
                inject_as: "TOKEN".to_string(),
                value: "secret".to_string(),
            },
        }],
        folder_bindings: vec![ProfileFolderBinding {
            project_id: "proj".to_string(),
            environment: "dev".to_string(),
            secret_path: "/".to_string(),
            inject_prefix: Some("F_".to_string()),
            synced_at: Utc::now(),
            snapshot: vec!["alpha".to_string()],
            infisical_profile: None,
        }],
    };
    let yaml = serde_yaml::to_string(&profile).unwrap();
    std::fs::create_dir_all(home.join("profiles")).unwrap();
    std::fs::write(home.join("profiles").join("readonly.yaml"), yaml).unwrap();

    let mut state = ClixState::from_home(home);
    state.config.active_profiles = vec!["readonly".to_string()];
    let (secret_bindings, folder_bindings) =
        clix_core::loader::active_profile_bindings(&state).unwrap();
    assert_eq!(secret_bindings.len(), 1);
    assert_eq!(folder_bindings.len(), 1);
    assert_eq!(secret_bindings[0].inject_as, "TOKEN");
    assert_eq!(folder_bindings[0].inject_prefix.as_deref(), Some("F_"));
}
