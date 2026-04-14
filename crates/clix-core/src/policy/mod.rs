pub mod evaluate;
pub use evaluate::{evaluate_policy, Decision, ExecutionContext};
use serde::{Deserialize, Serialize};
use crate::manifest::capability::SideEffectClass;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyBundle {
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
    #[serde(default)]
    pub default_action: PolicyAction,
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
