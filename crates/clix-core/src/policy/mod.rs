pub mod evaluate;
pub use evaluate::{evaluate_policy, Decision, ExecutionContext};
use serde::{Deserialize, Serialize};
use crate::manifest::capability::SideEffectClass;

fn default_deny() -> PolicyAction { PolicyAction::Deny }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyBundle {
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
    /// Fallback action when no rule matches. Defaults to Deny (fail-closed).
    /// Set to "allow" in policy.yaml to restore the old risk-based allow behavior.
    #[serde(default = "default_deny")]
    pub default_action: PolicyAction,
}

impl Default for PolicyBundle {
    fn default() -> Self {
        PolicyBundle { rules: vec![], default_action: PolicyAction::Deny }
    }
}

impl PolicyBundle {
    /// Construct a bundle that allows all unmatched capabilities (the pre-M4 default).
    /// Intended for use in tests and explicit opt-in configs — not the shipped default.
    pub fn allow_all() -> Self {
        PolicyBundle { rules: vec![], default_action: PolicyAction::Allow }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub env: Option<String>,
    #[serde(default)]
    pub risk: Option<String>,
    #[serde(default)]
    pub side_effect_class: Option<SideEffectClass>,
    pub action: PolicyAction,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PolicyAction {
    #[default]
    Allow,
    Deny,
    RequireApproval,
}
