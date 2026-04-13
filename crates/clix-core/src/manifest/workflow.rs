use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub steps: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub capability: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub on_failure: StepFailurePolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepFailurePolicy {
    #[default]
    Abort,
    Continue,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_workflow_yaml() {
        let yaml = "name: check-health\nversion: 1\nsteps:\n  - capability: kubectl.get-nodes\n    input: {}\n  - capability: kubectl.get-pods\n    input:\n      namespace: kube-system\n";
        let w: WorkflowManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(w.steps.len(), 2);
    }
}
