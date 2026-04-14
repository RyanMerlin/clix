use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::manifest::capability::{CapabilityManifest, RiskLevel};
use super::{PolicyAction, PolicyBundle, PolicyRule};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub env: String,
    pub cwd: PathBuf,
    pub user: String,
    pub profile: String,
    pub approver: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Decision {
    Allow,
    Deny { reason: String },
    RequireApproval { reason: String },
}

pub fn evaluate_policy(policy: &PolicyBundle, ctx: &ExecutionContext, cap: &CapabilityManifest) -> Decision {
    for rule in &policy.rules {
        if !rule_matches(rule, ctx, cap) { continue; }
        let reason = rule.reason.clone().unwrap_or_else(|| format!("matched policy rule for {}", cap.name));
        return match rule.action {
            PolicyAction::Allow => Decision::Allow,
            PolicyAction::Deny => Decision::Deny { reason },
            PolicyAction::RequireApproval => Decision::RequireApproval { reason },
        };
    }
    match cap.risk {
        RiskLevel::Low | RiskLevel::Medium => Decision::Allow,
        RiskLevel::High | RiskLevel::Critical => Decision::RequireApproval {
            reason: format!("{} risk capability requires approval", risk_label(&cap.risk)),
        },
    }
}

fn rule_matches(rule: &PolicyRule, ctx: &ExecutionContext, cap: &CapabilityManifest) -> bool {
    if let Some(ref name) = rule.capability { if name != &cap.name { return false; } }
    if let Some(ref profile) = rule.profile { if profile != &ctx.profile { return false; } }
    if let Some(ref env) = rule.env { if env != &ctx.env { return false; } }
    if let Some(ref risk) = rule.risk { if risk != &risk_label(&cap.risk) { return false; } }
    if let Some(ref sec) = rule.side_effect_class {
        if std::mem::discriminant(sec) != std::mem::discriminant(&cap.side_effect_class) { return false; }
    }
    true
}

fn risk_label(r: &RiskLevel) -> String {
    match r { RiskLevel::Low => "low", RiskLevel::Medium => "medium", RiskLevel::High => "high", RiskLevel::Critical => "critical" }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, CapabilityManifest, SideEffectClass};
    use crate::policy::{PolicyBundle, PolicyRule, PolicyAction};

    fn stub_cap(name: &str, risk: RiskLevel) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(), version: 1, description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk, side_effect_class: SideEffectClass::ReadOnly,
            sandbox_profile: None, isolation: Default::default(), approval_policy: None,
            input_schema: serde_json::json!({}), validators: vec![], credentials: vec![],
        }
    }

    fn ctx() -> ExecutionContext {
        ExecutionContext { env: "default".to_string(), cwd: PathBuf::from("/tmp"), user: "agent".to_string(), profile: "base".to_string(), approver: None }
    }

    #[test]
    fn test_allow_low_risk() {
        let policy = PolicyBundle::default();
        assert!(matches!(evaluate_policy(&policy, &ctx(), &stub_cap("sys.date", RiskLevel::Low)), Decision::Allow));
    }

    #[test]
    fn test_deny_by_name() {
        let mut policy = PolicyBundle::default();
        policy.rules.push(PolicyRule { capability: Some("bad.cmd".to_string()), action: PolicyAction::Deny, reason: Some("no".to_string()), ..Default::default() });
        assert!(matches!(evaluate_policy(&policy, &ctx(), &stub_cap("bad.cmd", RiskLevel::High)), Decision::Deny { .. }));
    }

    #[test]
    fn test_require_approval_high_risk() {
        let policy = PolicyBundle::default();
        assert!(matches!(evaluate_policy(&policy, &ctx(), &stub_cap("k8s.apply", RiskLevel::High)), Decision::RequireApproval { .. }));
    }
}
