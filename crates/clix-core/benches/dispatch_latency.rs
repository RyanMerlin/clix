/// Benchmark suite: gateway dispatch latency and worker-spawn overhead.
///
/// Targets (from the plan):
///   - gateway → builtin dispatch p50 < 1 ms   (no process overhead)
///   - cold worker spawn < 100 ms              (Linux only, requires clix-worker on PATH)
///
/// The builtin bench runs everywhere; the worker-spawn bench is skipped on non-Linux
/// or when clix-worker is not found next to the test binary / on PATH.
use criterion::{criterion_group, criterion_main, Criterion};
use std::path::PathBuf;
use clix_core::execution::run_capability;
use clix_core::manifest::capability::{Backend, CapabilityManifest, IsolationTier, RiskLevel, SideEffectClass};
use clix_core::policy::PolicyBundle;
use clix_core::receipts::ReceiptStore;
use clix_core::registry::CapabilityRegistry;
use clix_core::policy::evaluate::ExecutionContext;

fn date_cap() -> CapabilityManifest {
    CapabilityManifest {
        name:              "sys.date".to_string(),
        version:           1,
        description:       None,
        backend:           Backend::Builtin { name: "date".to_string() },
        risk:              RiskLevel::Low,
        side_effect_class: SideEffectClass::None,
        sandbox_profile:   None,
        isolation:         IsolationTier::None,
        approval_policy:   None,
        input_schema:      serde_json::json!({"type":"object","properties":{}}),
        validators:        vec![],
        credentials:       vec![],
    }
}

fn ctx() -> ExecutionContext {
    ExecutionContext {
        env:      "bench".to_string(),
        cwd:      PathBuf::from("."),
        user:     "bench".to_string(),
        profile:  "base".to_string(),
        approver: None,
    }
}

/// Builtin dispatch: policy check + in-process handler + receipt write.
/// This is the baseline; should be well under 1 ms.
fn bench_builtin_dispatch(c: &mut Criterion) {
    let registry = CapabilityRegistry::from_vec(vec![date_cap()]);
    let policy   = PolicyBundle::default();
    let store    = ReceiptStore::open(std::path::Path::new(":memory:")).unwrap();
    let input    = serde_json::json!({});

    c.bench_function("builtin_dispatch_sys_date", |b| {
        b.iter(|| {
            run_capability(
                &registry, &policy, None, &store, None,
                "sys.date", input.clone(), ctx(), &[],
            ).unwrap()
        })
    });
}

/// Policy deny path: evaluate → write denied receipt → return.
/// Should be similar to or faster than the allow path (no backend execution).
fn bench_policy_deny(c: &mut Criterion) {
    use clix_core::policy::{PolicyAction, PolicyRule};

    let registry = CapabilityRegistry::from_vec(vec![date_cap()]);
    let store    = ReceiptStore::open(std::path::Path::new(":memory:")).unwrap();
    let input    = serde_json::json!({});

    let mut policy = PolicyBundle::default();
    policy.rules.push(PolicyRule {
        capability: Some("sys.date".to_string()),
        action:     PolicyAction::Deny,
        reason:     Some("bench deny".to_string()),
        ..Default::default()
    });

    c.bench_function("policy_deny_sys_date", |b| {
        b.iter(|| {
            run_capability(
                &registry, &policy, None, &store, None,
                "sys.date", input.clone(), ctx(), &[],
            ).unwrap()
        })
    });
}

/// Worker registry construction (no spawning).
fn bench_registry_new(c: &mut Criterion) {
    use clix_core::execution::worker_registry::WorkerRegistry;

    c.bench_function("worker_registry_new", |b| {
        b.iter(|| {
            let reg = WorkerRegistry::new_with_broker(
                PathBuf::from("clix-worker"), 300, None,
            );
            // Ensure not optimized away
            std::hint::black_box(reg.worker_count());
        })
    });
}

criterion_group!(benches, bench_builtin_dispatch, bench_policy_deny, bench_registry_new);
criterion_main!(benches);
