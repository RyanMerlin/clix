use serde::{Deserialize, Serialize};
use super::capability::IsolationTier;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub settings: serde_json::Value,
    /// Profile-wide isolation defaults. Individual capabilities can override.
    #[serde(default)]
    pub isolation_defaults: IsolationDefaults,
}

/// Profile-level defaults that apply to all capabilities unless overridden at capability level.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IsolationDefaults {
    /// Minimum isolation tier; capabilities with a weaker tier are upgraded to this.
    #[serde(default)]
    pub minimum_tier: IsolationTier,
    /// Seconds an idle warm worker is kept alive before being reaped. Default 300.
    #[serde(default = "default_worker_idle_ttl")]
    pub worker_idle_ttl_secs: u64,
    /// Maximum memory in MiB for any worker in this profile. Default 512.
    #[serde(default = "default_memory_mib")]
    pub worker_memory_mib: u64,
    /// Egress allowlist inherited by all capabilities unless they define their own.
    #[serde(default)]
    pub egress_allowlist: Vec<String>,
}

impl Default for IsolationDefaults {
    fn default() -> Self {
        IsolationDefaults {
            minimum_tier: IsolationTier::WarmWorker,
            worker_idle_ttl_secs: default_worker_idle_ttl(),
            worker_memory_mib: default_memory_mib(),
            egress_allowlist: vec![],
        }
    }
}

fn default_worker_idle_ttl() -> u64 { 300 }
fn default_memory_mib() -> u64 { 512 }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_profile_yaml() {
        let yaml = "name: kubectl-observe\nversion: 1\ncapabilities: [kubectl.get-pods]\n";
        let p: ProfileManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.name, "kubectl-observe");
        assert_eq!(p.capabilities.len(), 1);
    }
}
