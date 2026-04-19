//! Fluent CapabilityManifest and CapabilityRegistry builders.

use clix_core::manifest::capability::{
    Backend, CapabilityManifest, IsolationTier, RiskLevel, SideEffectClass,
};
use clix_core::registry::CapabilityRegistry;

/// Build a minimal `CapabilityManifest` using the `date` builtin backend.
pub fn builtin(name: &str) -> CapabilityManifest {
    CapabilityManifest {
        name:             name.to_string(),
        version:          1,
        description:      Some(format!("Test capability {name}")),
        backend:          Backend::Builtin { name: "date".to_string() },
        risk:             RiskLevel::Low,
        side_effect_class: SideEffectClass::ReadOnly,
        sandbox_profile:  None,
        isolation:        IsolationTier::None,
        approval_policy:  None,
        input_schema:     serde_json::json!({"type":"object","properties":{}}),
        validators:       vec![],
        credentials:      vec![],
        argv_pattern:     None,
    }
}

/// Build a capability with a specific `SideEffectClass`.
pub fn with_side_effect(name: &str, side_effect: SideEffectClass) -> CapabilityManifest {
    CapabilityManifest { side_effect_class: side_effect, ..builtin(name) }
}

/// Build a `CapabilityRegistry` from a list of manifests.
pub fn registry(caps: Vec<CapabilityManifest>) -> CapabilityRegistry {
    CapabilityRegistry::from_vec(caps)
}
