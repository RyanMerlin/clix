use anyhow::Result;
use clix_core::sandbox::sandbox_enforced;
use clix_core::state::{home_dir, ClixState};
use clix_core::loader::build_registry;
use clix_core::secrets::test_connectivity;
use crate::output::{print_json, print_kv};

pub fn run(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let enforced = sandbox_enforced();

    let registry = build_registry(&state).unwrap_or_default();
    let cap_count = registry.all().len();

    let pack_count = if state.packs_dir.exists() {
        std::fs::read_dir(&state.packs_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .filter(|e| e.path().join("pack.yaml").exists())
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    let active_profile = state.config.active_profiles.first()
        .cloned()
        .unwrap_or_else(|| "none".to_string());

    // Check Infisical connectivity
    let infisical_status = if let Some(ref cfg) = state.config.infisical {
        let report = test_connectivity(cfg);
        if report.auth_ok {
            format!("connected ({}ms)", report.latency_ms)
        } else {
            format!("error: {}", report.error.as_deref().unwrap_or("unknown"))
        }
    } else {
        "not configured".to_string()
    };

    // Check if broker socket exists
    let broker_socket = {
        let p = std::path::PathBuf::from(
            std::env::var("CLIX_BROKER_SOCKET")
                .unwrap_or_else(|_| "/tmp/clix-broker.sock".to_string())
        );
        p.exists()
    };

    if json {
        print_json(&serde_json::json!({
            "broker_up": broker_socket,
            "sandbox_enforced": enforced,
            "active_profile": active_profile,
            "pack_count": pack_count,
            "capability_count": cap_count,
            "home": state.home,
            "infisical": infisical_status,
        }));
    } else {
        print_kv(&[
            ("broker",       if broker_socket { "up" } else { "not running" }.to_string()),
            ("sandbox",      if enforced { "enforced" } else { "not enforced" }.to_string()),
            ("profile",      active_profile),
            ("packs",        pack_count.to_string()),
            ("capabilities", cap_count.to_string()),
            ("home",         state.home.display().to_string()),
            ("infisical",    infisical_status),
        ]);
    }
    Ok(())
}
