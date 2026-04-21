use anyhow::{anyhow, Result};
use clix_core::execution::run_capability;
use clix_core::loader::{build_registry, load_policy};
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::receipts::ReceiptStore;
use clix_core::state::{home_dir, ClixState};
use crate::output::print_json;

pub fn run(capability: &str, input_pairs: &[String], json: bool, dry_run: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let registry = build_registry(&state)?;
    let policy = load_policy(&state)?;
    let input = parse_input_pairs(input_pairs)?;

    // Validate inputs against the capability's JSON Schema before executing.
    let cap = registry.get(capability)
        .ok_or_else(|| anyhow!("capability not found: {capability}"))?;
    validate_inputs(&input, &cap.input_schema, json)?;

    if dry_run {
        // Evaluate policy without executing — no receipt written.
        let ctx = ExecutionContext {
            env: state.config.default_env.clone(),
            cwd: state.config.workspace_root.clone().unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            }),
            user: whoami::username(),
            profile: state.config.active_profiles.first().cloned().unwrap_or_else(|| "default".to_string()),
            approver: None,
        };
        let decision = clix_core::policy::evaluate::evaluate_policy(&policy, &ctx, cap);
        let would_run = matches!(decision, clix_core::policy::evaluate::Decision::Allow);
        let result = serde_json::json!({
            "would_run": would_run,
            "policy": format!("{:?}", decision),
            "capability": capability,
            "isolation_tier": format!("{:?}", cap.isolation),
            "inputs": input,
        });
        if json {
            print_json(&result);
        } else {
            println!("dry-run: {} (policy={:?}, isolation={:?})",
                capability, decision, cap.isolation);
        }
        return Ok(());
    }

    let store = ReceiptStore::open(&state.receipts_db)?;
    let ctx = ExecutionContext {
        env: state.config.default_env.clone(),
        cwd: state.config.workspace_root.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        }),
        user: whoami::username(),
        profile: state.config.active_profiles.first().cloned().unwrap_or_else(|| "default".to_string()),
        approver: None,
    };
    let outcome = run_capability(&registry, &policy, &state.config.infisical(), &store, None, capability, input, ctx, &[])
        .map_err(|e| anyhow!("{e}"))?;
    if json {
        // Always emit the full outcome struct under --json for predictable agent parsing.
        print_json(&outcome);
    } else if outcome.ok {
        println!("ok — receipt {}", outcome.receipt_id);
        if let Some(result) = &outcome.result {
            if let Some(stdout) = result["stdout"].as_str() {
                if !stdout.is_empty() { print!("{stdout}"); }
            } else if let Some(date) = result["date"].as_str() {
                println!("{date}");
            } else if let Some(output) = result["output"].as_str() {
                println!("{output}");
            }
        }
    } else if outcome.approval_required {
        eprintln!("approval required — receipt {}", outcome.receipt_id);
        std::process::exit(2);
    } else {
        eprintln!("denied: {}", outcome.reason.unwrap_or_default());
        std::process::exit(1);
    }
    Ok(())
}

/// Validate that `inputs` only contains keys declared in the capability's JSON Schema.
/// Returns a structured error (and optionally prints JSON) if unknown keys are present.
fn validate_inputs(inputs: &serde_json::Value, schema: &serde_json::Value, json: bool) -> Result<()> {
    let props = schema.get("properties");
    let Some(props) = props else { return Ok(()); };
    let Some(obj) = inputs.as_object() else { return Ok(()); };

    let declared: Vec<&str> = props.as_object()
        .map(|m| m.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();

    let unknown: Vec<&str> = obj.keys()
        .map(|k| k.as_str())
        .filter(|k| !declared.contains(k))
        .collect();

    if !unknown.is_empty() {
        let msg = serde_json::json!({
            "error": format!("unknown input(s): {}", unknown.join(", ")),
            "expected": declared,
        });
        if json {
            eprintln!("{}", serde_json::to_string_pretty(&msg)?);
        } else {
            eprintln!("error: unknown input(s): {}", unknown.join(", "));
            eprintln!("expected: {}", declared.join(", "));
        }
        std::process::exit(2);
    }
    Ok(())
}

pub fn parse_input_pairs(pairs: &[String]) -> Result<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for pair in pairs {
        let (key, value) = pair.split_once('=').ok_or_else(|| anyhow!("input must be key=value, got: {pair}"))?;
        let v: serde_json::Value = serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        map.insert(key.to_string(), v);
    }
    Ok(serde_json::Value::Object(map))
}
