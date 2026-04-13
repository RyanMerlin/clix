use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub backend: Backend,
    #[serde(default)]
    pub risk: RiskLevel,
    #[serde(default)]
    pub side_effect_class: SideEffectClass,
    #[serde(default)]
    pub sandbox_profile: Option<String>,
    #[serde(default)]
    pub approval_policy: Option<String>,
    #[serde(default = "default_schema")]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub validators: Vec<Validator>,
    #[serde(default)]
    pub credentials: Vec<CredentialSource>,
}

fn default_schema() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Backend {
    #[serde(rename = "subprocess")]
    Subprocess {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        cwd_from_input: Option<String>,
    },
    #[serde(rename = "builtin")]
    Builtin { name: String },
    #[serde(rename = "remote")]
    Remote { url: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SideEffectClass {
    #[serde(rename = "none")]
    #[default]
    None,
    #[serde(rename = "readOnly")]
    ReadOnly,
    Additive,
    Mutating,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Validator {
    #[serde(rename = "type")]
    pub kind: ValidatorKind,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidatorKind {
    RequiredPath,
    DenyArgs,
    RequiredInputKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CredentialSource {
    #[serde(rename = "env")]
    Env { env_var: String, inject_as: String },
    #[serde(rename = "literal")]
    Literal { value: String, inject_as: String },
    #[serde(rename = "infisical")]
    Infisical {
        #[serde(flatten)]
        secret_ref: InfisicalRef,
        inject_as: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfisicalRef {
    pub secret_name: String,
    #[serde(default)]
    pub project_id: Option<String>,
    pub environment: String,
    #[serde(default = "default_secret_path")]
    pub secret_path: String,
}

fn default_secret_path() -> String { "/".to_string() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_roundtrip_json() {
        let json = serde_json::json!({
            "name": "kubectl.get-pods",
            "version": 1,
            "backend": { "type": "subprocess", "command": "kubectl", "args": ["get", "pods"] },
            "risk": "low",
            "sideEffectClass": "readOnly",
            "inputSchema": { "type": "object", "properties": {} }
        });
        let cap: CapabilityManifest = serde_json::from_value(json).unwrap();
        assert_eq!(cap.name, "kubectl.get-pods");
        assert!(matches!(cap.risk, RiskLevel::Low));
        match &cap.backend {
            Backend::Subprocess { command, .. } => assert_eq!(command, "kubectl"),
            _ => panic!("expected subprocess"),
        }
    }

    #[test]
    fn test_capability_roundtrip_yaml() {
        let yaml = "name: gcloud.list\nversion: 1\nbackend:\n  type: subprocess\n  command: gcloud\n  args: [\"projects\", \"list\"]\nrisk: low\nsideEffectClass: readOnly\ninputSchema:\n  type: object\n  properties: {}\n";
        let cap: CapabilityManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cap.name, "gcloud.list");
    }
}
