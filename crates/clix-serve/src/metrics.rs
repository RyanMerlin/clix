use prometheus::{IntCounterVec, Opts, Registry, TextEncoder, Encoder};
use std::sync::OnceLock;

static REGISTRY: OnceLock<Registry> = OnceLock::new();
static CALLS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();
static DENIALS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();
static ERRORS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();

pub fn init() {
    let registry = Registry::new();
    let calls = IntCounterVec::new(
        Opts::new("clix_capability_calls_total", "Total capability calls"),
        &["capability", "status"],
    ).unwrap();
    let denials = IntCounterVec::new(
        Opts::new("clix_capability_denials_total", "Capability calls denied by policy"),
        &["capability"],
    ).unwrap();
    let errors = IntCounterVec::new(
        Opts::new("clix_capability_errors_total", "Capability calls that errored"),
        &["capability"],
    ).unwrap();
    registry.register(Box::new(calls.clone())).unwrap();
    registry.register(Box::new(denials.clone())).unwrap();
    registry.register(Box::new(errors.clone())).unwrap();
    let _ = CALLS_TOTAL.set(calls);
    let _ = DENIALS_TOTAL.set(denials);
    let _ = ERRORS_TOTAL.set(errors);
    let _ = REGISTRY.set(registry);
}

pub fn record_call(capability: &str, status: &str) {
    if let Some(c) = CALLS_TOTAL.get() { c.with_label_values(&[capability, status]).inc(); }
}

pub fn record_denial(capability: &str) {
    if let Some(c) = DENIALS_TOTAL.get() { c.with_label_values(&[capability]).inc(); }
}

pub fn record_error(capability: &str) {
    if let Some(c) = ERRORS_TOTAL.get() { c.with_label_values(&[capability]).inc(); }
}

pub fn render() -> String {
    let encoder = TextEncoder::new();
    let mut buf = Vec::new();
    if let Some(r) = REGISTRY.get() {
        let mf = r.gather();
        encoder.encode(&mf, &mut buf).unwrap_or_default();
    }
    String::from_utf8(buf).unwrap_or_default()
}
