use std::collections::HashMap;
use crate::manifest::capability::CapabilityManifest;
use crate::manifest::workflow::WorkflowManifest;

#[derive(Debug, Default, Clone)]
pub struct CapabilityRegistry {
    caps: HashMap<String, CapabilityManifest>,
}

impl CapabilityRegistry {
    pub fn from_vec(caps: Vec<CapabilityManifest>) -> Self {
        let mut reg = Self::default();
        for cap in caps { reg.caps.insert(cap.name.clone(), cap); }
        reg
    }
    pub fn get(&self, name: &str) -> Option<&CapabilityManifest> { self.caps.get(name) }
    pub fn all(&self) -> Vec<&CapabilityManifest> {
        let mut v: Vec<_> = self.caps.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
    pub fn insert(&mut self, cap: CapabilityManifest) { self.caps.insert(cap.name.clone(), cap); }
}

#[derive(Debug, Default, Clone)]
pub struct WorkflowRegistry {
    workflows: HashMap<String, WorkflowManifest>,
}

impl WorkflowRegistry {
    pub fn from_vec(workflows: Vec<WorkflowManifest>) -> Self {
        let mut reg = Self::default();
        for wf in workflows { reg.workflows.insert(wf.name.clone(), wf); }
        reg
    }
    pub fn get(&self, name: &str) -> Option<&WorkflowManifest> { self.workflows.get(name) }
    pub fn all(&self) -> Vec<&WorkflowManifest> {
        let mut v: Vec<_> = self.workflows.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, RiskLevel, SideEffectClass};

    fn make_cap(name: &str) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(), version: 1, description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low, side_effect_class: SideEffectClass::None,
            sandbox_profile: None, approval_policy: None,
            input_schema: serde_json::json!({}), validators: vec![], credentials: vec![],
        }
    }

    #[test]
    fn test_registry_get() {
        let reg = CapabilityRegistry::from_vec(vec![make_cap("sys.date"), make_cap("sys.echo")]);
        assert!(reg.get("sys.date").is_some());
        assert!(reg.get("missing").is_none());
        assert_eq!(reg.all().len(), 2);
    }
}
