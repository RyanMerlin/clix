use anyhow::Result;
use clix_core::execution::worker_registry::WorkerRegistry;
use clix_core::sandbox::sandbox_enforced;
use clix_core::state::{home_dir, ClixState};
use clix_core::loader::build_registry;
use clix_core::secrets::test_connectivity;
use clix_core::receipts::ReceiptStore;
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

    // Live broker ping
    let (broker_label, broker_status_str) = check_broker();

    // Worker binary + isolation readiness
    let (worker_label, worker_status_str) = check_worker();

    // Receipt stats
    let receipt_stats = ReceiptStore::open(&state.receipts_db)
        .ok()
        .and_then(|s| s.count_by_status().ok());
    let (r_total, r_allowed, r_denied, r_failed, _r_pending) =
        receipt_stats.unwrap_or((0, 0, 0, 0, 0));
    let receipts_summary = format!(
        "{r_total} total  ({r_allowed} allowed · {r_denied} denied · {r_failed} failed)"
    );

    let sandbox_mode = if enforced { "enforced" } else { "policy-only" };
    let sandbox_detail = sandbox_detail(enforced);
    if json {
        print_json(&serde_json::json!({
            "broker_up": broker_label.starts_with('✓'),
            "broker_status": broker_status_str,
            "worker_up": worker_label.starts_with('✓'),
            "worker_status": worker_status_str,
            "sandbox": sandbox_mode,
            "sandbox_enforced": enforced,
            "sandbox_detail": sandbox_detail,
            "active_profile": active_profile,
            "pack_count": pack_count,
            "capability_count": cap_count,
            "home": state.home,
            "infisical": infisical_status,
            "receipts": {
                "total": r_total,
                "allowed": r_allowed,
                "denied": r_denied,
                "failed": r_failed,
            },
        }));
    } else {
        print_kv(&[
            ("broker",       format!("{broker_label}: {broker_status_str}")),
            ("worker",       format!("{worker_label}: {worker_status_str}")),
            ("sandbox",      format!("{sandbox_mode} ({sandbox_detail})")),
            ("profile",      active_profile),
            ("packs",        pack_count.to_string()),
            ("capabilities", cap_count.to_string()),
            ("home",         state.home.display().to_string()),
            ("infisical",    infisical_status),
            ("receipts",     receipts_summary),
        ]);
    }
    Ok(())
}

fn check_worker() -> (&'static str, String) {
    let binary = WorkerRegistry::locate_worker_binary();
    if !binary.exists() {
        return ("✗ worker", format!("clix-worker not found (expected alongside clix at {})", binary.display()));
    }

    #[cfg(target_os = "linux")]
    {
        let apparmor_val = std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
            .unwrap_or_default();
        if apparmor_val.trim() == "1" {
            return ("⚠ worker", format!(
                "binary found but AppArmor blocks user namespaces\n\
                 \x20      Fix (recommended): \
                 sudo cp assets/apparmor/clix-worker /etc/apparmor.d/clix-worker && \
                 sudo apparmor_parser -r /etc/apparmor.d/clix-worker\n\
                 \x20      Fix (global, less safe): \
                 sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0"
            ));
        }
    }

    ("✓ worker", format!("ready ({})", binary.display()))
}

fn sandbox_detail(enforced: bool) -> String {
    #[cfg(target_os = "linux")]
    { if enforced { "landlock".to_string() } else { "landlock unavailable".to_string() } }
    #[cfg(target_os = "macos")]
    {
        let available = clix_core::sandbox::macos::sandbox_exec_available();
        if available { "sandbox-exec (BETA)".to_string() } else { "sandbox-exec not found".to_string() }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    { "not supported on this platform".to_string() }
}

fn check_broker() -> (&'static str, String) {
    let socket_path = std::env::var("CLIX_BROKER_SOCKET")
        .unwrap_or_else(|_| "/tmp/clix-broker.sock".to_string());

    use std::os::unix::net::UnixStream;
    use std::io::{BufRead, BufReader, Write};
    use std::time::Instant;

    let start = Instant::now();
    match UnixStream::connect(&socket_path) {
        Err(e) => ("✗ broker", format!("socket unreachable: {e}")),
        Ok(mut stream) => {
            let _ = stream.write_all(b"{\"type\":\"ping\"}\n");
            let reader = BufReader::new(&stream);
            match reader.lines().next() {
                Some(Ok(line)) if line.contains("pong") =>
                    ("✓ broker", format!("{}ms", start.elapsed().as_millis())),
                Some(Ok(line)) => ("✗ broker", format!("unexpected: {line}")),
                _ => ("✗ broker", "no response".to_string()),
            }
        }
    }
}
