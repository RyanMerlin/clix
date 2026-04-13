use std::collections::HashMap;
use crate::manifest::capability::CapabilityManifest;
use crate::manifest::workflow::WorkflowManifest;

#[derive(Debug, Clone)]
pub struct NamespaceStub {
    pub key: String,
    pub count: usize,
}

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

    /// Returns the top-level namespace group key for a capability name.
    /// - 0 dots → the name itself
    /// - 1 dot  → everything before the dot  ("system.date" → "system")
    /// - 2+ dots → first two segments         ("gcloud.aiplatform.models.list" → "gcloud.aiplatform")
    pub fn group_key(name: &str) -> String {
        let dots: Vec<usize> = name.match_indices('.').map(|(i, _)| i).collect();
        match dots.len() {
            0 => name.to_string(),
            1 => name[..dots[0]].to_string(),
            _ => name[..dots[1]].to_string(),
        }
    }

    /// Returns namespace stubs grouped by `group_key`, sorted by key.
    pub fn namespaces(&self) -> Vec<NamespaceStub> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for name in self.caps.keys() {
            *counts.entry(Self::group_key(name)).or_insert(0) += 1;
        }
        let mut stubs: Vec<NamespaceStub> = counts
            .into_iter()
            .map(|(key, count)| NamespaceStub { key, count })
            .collect();
        stubs.sort_by(|a, b| a.key.cmp(&b.key));
        stubs
    }

    /// Returns all capabilities whose `group_key` equals `namespace`.
    /// Each capability belongs to exactly one namespace group; this returns the members of that group.
    /// "gcloud" returns only `gcloud.*` leaves, not `gcloud.aiplatform.*` sub-namespace caps.
    pub fn by_namespace(&self, namespace: &str) -> Vec<&CapabilityManifest> {
        let mut v: Vec<_> = self.caps.values()
            .filter(|c| Self::group_key(&c.name) == namespace)
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
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

    #[test]
    fn test_group_key_one_dot() {
        assert_eq!(CapabilityRegistry::group_key("system.date"), "system");
        assert_eq!(CapabilityRegistry::group_key("gcloud.list-projects"), "gcloud");
        assert_eq!(CapabilityRegistry::group_key("nodot"), "nodot");
    }

    #[test]
    fn test_group_key_two_plus_dots() {
        assert_eq!(CapabilityRegistry::group_key("gcloud.aiplatform.models.list"), "gcloud.aiplatform");
        assert_eq!(CapabilityRegistry::group_key("gcloud.aiplatform.endpoints.describe"), "gcloud.aiplatform");
        assert_eq!(CapabilityRegistry::group_key("a.b.c"), "a.b");
    }

    #[test]
    fn test_namespaces() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("system.date"),
            make_cap("system.echo"),
            make_cap("gcloud.aiplatform.models.list"),
            make_cap("gcloud.aiplatform.endpoints.list"),
        ]);
        let stubs = reg.namespaces();
        assert_eq!(stubs.len(), 2);
        let sys = stubs.iter().find(|s| s.key == "system").unwrap();
        assert_eq!(sys.count, 2);
        let gca = stubs.iter().find(|s| s.key == "gcloud.aiplatform").unwrap();
        assert_eq!(gca.count, 2);
    }

    #[test]
    fn test_by_namespace() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("gcloud.aiplatform.models.list"),
            make_cap("gcloud.aiplatform.endpoints.list"),
            make_cap("system.date"),
        ]);
        let matched = reg.by_namespace("gcloud.aiplatform");
        assert_eq!(matched.len(), 2);
        assert!(matched.iter().all(|c| c.name.starts_with("gcloud.aiplatform.")));

        let gcloud_only = reg.by_namespace("gcloud");
        assert_eq!(gcloud_only.len(), 0);
    }
}
