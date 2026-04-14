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
    /// Structured sandbox policy for this capability. Controls seccomp, fs, network, and cgroup limits.
    #[serde(default)]
    pub sandbox_profile: Option<SandboxProfile>,
    /// Which isolation tier to run this capability in (defaults to warm_worker on Linux).
    #[serde(default)]
    pub isolation: IsolationTier,
    #[serde(default)]
    pub approval_policy: Option<String>,
    #[serde(default = "default_schema")]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub validators: Vec<Validator>,
    #[serde(default)]
    pub credentials: Vec<CredentialSource>,
}

/// Which isolation boundary to enforce when executing this capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IsolationTier {
    /// No isolation — used for builtins running in-process.
    #[serde(rename = "none")]
    None,
    /// Default: long-running worker process jailed with Linux namespaces, Landlock, seccomp, and
    /// cgroup v2. Dispatch latency <5 ms after warm-up.
    #[default]
    #[serde(rename = "warm_worker")]
    WarmWorker,
    /// Firecracker microVM pool. Strongest boundary; opt-in for high-risk CLIs.
    /// Requires `feature = "firecracker"` and KVM.
    #[serde(rename = "firecracker")]
    Firecracker,
}

/// Structured sandbox constraints attached to a capability or profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxProfile {
    /// Additional seccomp syscall allowlist on top of the built-in safe baseline.
    /// Names are Linux syscall names (e.g. "openat", "read"). Empty = use baseline only.
    #[serde(default)]
    pub extra_syscalls: Vec<String>,
    /// Filesystem policy for what paths the worker may access.
    #[serde(default)]
    pub fs: FsPolicy,
    /// Network egress policy. Default is deny-all.
    #[serde(default)]
    pub network: NetworkPolicy,
    /// Resource limits for the worker's cgroup.
    #[serde(default)]
    pub limits: CgroupLimits,
}

/// What parts of the host filesystem the worker may access.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsPolicy {
    /// Additional read-only bind mounts (host paths) to expose inside the jail. The CLI binary
    /// and its dynamic libraries are always bound automatically.
    #[serde(default)]
    pub extra_ro_bind: Vec<String>,
    /// Additional read-write bind mounts (host paths). Use sparingly.
    #[serde(default)]
    pub extra_rw_bind: Vec<String>,
    /// If true, expose /tmp from the host (default: isolated tmpfs /tmp).
    #[serde(default)]
    pub share_host_tmp: bool,
}

/// Network egress rules for the jailed worker.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPolicy {
    /// Explicit host:port allowlist for outbound TCP connections. If empty, all network is denied.
    #[serde(default)]
    pub egress_allowlist: Vec<String>,
}

/// cgroup v2 resource constraints for the jailed worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CgroupLimits {
    /// Maximum RSS in MiB. Default 512.
    #[serde(default = "default_memory_mib")]
    pub memory_mib: u64,
    /// Maximum number of PIDs inside the worker. Default 64.
    #[serde(default = "default_max_pids")]
    pub max_pids: u64,
    /// CPU weight (relative, range 1–10000). Default 100.
    #[serde(default = "default_cpu_weight")]
    pub cpu_weight: u64,
}

impl Default for CgroupLimits {
    fn default() -> Self {
        CgroupLimits { memory_mib: default_memory_mib(), max_pids: default_max_pids(), cpu_weight: default_cpu_weight() }
    }
}

fn default_memory_mib() -> u64 { 512 }
fn default_max_pids() -> u64 { 64 }
fn default_cpu_weight() -> u64 { 100 }

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
        assert_eq!(cap.isolation, IsolationTier::WarmWorker);
        match &cap.backend {
            Backend::Subprocess { command, .. } => assert_eq!(command, "kubectl"),
            _ => panic!("expected subprocess"),
        }
    }

    #[test]
    fn test_isolation_tier_serde() {
        let json = serde_json::json!({"isolation": "firecracker"});
        let cap: serde_json::Value = json;
        let tier: IsolationTier = serde_json::from_value(cap["isolation"].clone()).unwrap();
        assert_eq!(tier, IsolationTier::Firecracker);
        let default: IsolationTier = serde_json::from_value(serde_json::json!("warm_worker")).unwrap();
        assert_eq!(default, IsolationTier::WarmWorker);
    }

    #[test]
    fn test_sandbox_profile_defaults() {
        let p = SandboxProfile::default();
        assert_eq!(p.limits.memory_mib, 512);
        assert_eq!(p.limits.max_pids, 64);
        assert!(p.network.egress_allowlist.is_empty());
    }

    #[test]
    fn test_capability_roundtrip_yaml() {
        let yaml = "name: gcloud.list\nversion: 1\nbackend:\n  type: subprocess\n  command: gcloud\n  args: [\"projects\", \"list\"]\nrisk: low\nsideEffectClass: readOnly\ninputSchema:\n  type: object\n  properties: {}\n";
        let cap: CapabilityManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cap.name, "gcloud.list");
    }
}
