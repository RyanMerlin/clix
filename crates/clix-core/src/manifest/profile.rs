use serde::{Deserialize, Deserializer, Serialize};
use chrono::{DateTime, Utc};
use super::capability::{CredentialSource, IsolationTier};

fn deser_cap_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    struct CapListVisitor;
    impl<'de> serde::de::Visitor<'de> for CapListVisitor {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "a list of capability names or objects")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
            let mut out = Vec::new();
            while let Some(v) = seq.next_element::<serde_json::Value>()? {
                let name = match v {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Object(m) => m
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                        .ok_or_else(|| serde::de::Error::custom("capability object missing 'name'"))?,
                    _ => return Err(serde::de::Error::custom("unexpected capability entry")),
                };
                out.push(name);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(CapListVisitor)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deser_cap_list")]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub settings: serde_json::Value,
    /// Profile-wide isolation defaults. Individual capabilities can override.
    #[serde(default)]
    pub isolation_defaults: IsolationDefaults,
    /// Secret bindings for this profile. Override capability-declared credential sources at execution time.
    #[serde(default)]
    pub secret_bindings: Vec<ProfileSecretBinding>,
    /// Folder-level bindings expand an entire Infisical path at execution time.
    #[serde(default)]
    pub folder_bindings: Vec<ProfileFolderBinding>,
}

/// Binds an entire Infisical folder path to a profile with a snapshot of secret names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFolderBinding {
    pub project_id: String,
    pub environment: String,
    pub secret_path: String,
    #[serde(default)]
    pub inject_prefix: Option<String>,
    pub synced_at: DateTime<Utc>,
    /// Secret names captured at sync time.
    pub snapshot: Vec<String>,
    /// Which named Infisical account profile to use when resolving. None = active profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infisical_profile: Option<String>,
}

/// Binds an environment variable name to a concrete credential source at the profile level.
/// At execution time this overrides any same-named binding declared by the capability itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSecretBinding {
    /// The env var name that will be injected into the process (matches `inject_as` in CredentialSource).
    pub inject_as: String,
    /// Where to source the value.
    pub source: CredentialSource,
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
