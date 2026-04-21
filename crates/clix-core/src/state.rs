use std::collections::BTreeMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::error::Result;
use crate::storage::{StorageRef, default_storage};

pub fn home_dir() -> PathBuf {
    if let Ok(v) = std::env::var("CLIX_HOME") {
        return PathBuf::from(v);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".clix")
}

#[derive(Clone)]
pub struct ClixState {
    pub home: PathBuf,
    pub config_path: PathBuf,
    pub policy_path: PathBuf,
    pub packs_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub capabilities_dir: PathBuf,
    pub workflows_dir: PathBuf,
    pub receipts_db: PathBuf,
    pub bundles_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config: ClixConfig,
    /// Storage backend — always `FsStorage` in production; swap for `MemStorage`
    /// in tests.
    pub storage: StorageRef,
}

impl std::fmt::Debug for ClixState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClixState")
            .field("home", &self.home)
            .field("config_path", &self.config_path)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ClixState {
    pub fn from_home(home: PathBuf) -> Self {
        Self::from_home_with_storage(home, default_storage())
    }

    pub fn from_home_with_storage(home: PathBuf, storage: StorageRef) -> Self {
        ClixState {
            config_path:      home.join("config.yaml"),
            policy_path:      home.join("policy.yaml"),
            packs_dir:        home.join("packs"),
            profiles_dir:     home.join("profiles"),
            capabilities_dir: home.join("capabilities"),
            workflows_dir:    home.join("workflows"),
            receipts_db:      home.join("receipts.db"),
            bundles_dir:      home.join("bundles"),
            cache_dir:        home.join("cache"),
            config:           ClixConfig::default(),
            storage,
            home,
        }
    }

    pub fn load(home: PathBuf) -> Result<Self> {
        Self::load_with_storage(home, default_storage())
    }

    pub fn load_with_storage(home: PathBuf, storage: StorageRef) -> Result<Self> {
        let mut state = Self::from_home_with_storage(home, storage);
        if state.storage.exists(&state.config_path) {
            let content = state.storage.read_to_string(&state.config_path)?;
            state.config = serde_yaml::from_str(&content)?;
        }
        // Promote legacy single-entry infisical field to named profiles map
        migrate_infisical_config(&mut state.config);
        #[cfg(target_os = "linux")]
        crate::secrets::keyring::merge_keyring_into_config(&mut state.config);
        Ok(state)
    }

    /// Write config.yaml and enforce 0600 permissions on Linux.
    pub fn save_config(&self) -> Result<()> {
        let yaml = serde_yaml::to_string(&self.config)?;
        self.storage.write(&self.config_path, yaml.as_bytes())?;
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&self.config_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&self.config_path, perms);
            }
        }
        Ok(())
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.home,
            &self.packs_dir,
            &self.profiles_dir,
            &self.capabilities_dir,
            &self.workflows_dir,
            &self.bundles_dir,
            &self.cache_dir,
        ] {
            self.storage.mkdir_p(dir)?;
        }
        Ok(())
    }
}

/// Migrate legacy `infisical: Option<InfisicalConfig>` into the named-profile map.
/// Runs every load; is a no-op once the map is populated.
fn migrate_infisical_config(config: &mut ClixConfig) {
    if config.infisical_profiles.is_empty() {
        if let Some(legacy) = config.infisical.take() {
            config.infisical_profiles.insert("default".to_string(), legacy);
            if config.active_infisical.is_none() {
                config.active_infisical = Some("default".to_string());
            }
        }
    } else {
        // Map is already populated; clear any stale legacy field so it doesn't re-appear on save.
        config.infisical = None;
    }
}

// ─── config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClixConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub approval_mode: ApprovalMode,
    #[serde(default = "default_env")]
    pub default_env: String,
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub active_profiles: Vec<String>,

    /// Named Infisical account profiles. Use `active_infisical` to select the default.
    #[serde(default)]
    pub infisical_profiles: BTreeMap<String, InfisicalConfig>,
    /// Name of the currently active Infisical profile (key into `infisical_profiles`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_infisical: Option<String>,
    /// Legacy single-profile field — migrated to `infisical_profiles["default"]` on first load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infisical: Option<InfisicalConfig>,

    #[serde(default)]
    pub approval_gate: Option<ApprovalGateConfig>,
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Git remote URL for syncing ~/.clix (e.g. "https://github.com/user/clix-merlin")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_remote: Option<String>,
    /// Branch to sync against (default: "main")
    #[serde(default = "default_git_branch")]
    pub git_branch: String,
}

impl ClixConfig {
    /// Returns a resolver over the named Infisical profiles.
    pub fn infisical(&self) -> InfisicalProfiles<'_> {
        InfisicalProfiles {
            profiles: &self.infisical_profiles,
            active: self.active_infisical.as_deref(),
        }
    }
}

impl Default for ClixConfig {
    fn default() -> Self {
        ClixConfig {
            schema_version: 1,
            approval_mode: ApprovalMode::Interactive,
            default_env: "default".to_string(),
            workspace_root: None,
            active_profiles: vec![],
            infisical_profiles: BTreeMap::new(),
            active_infisical: None,
            infisical: None,
            approval_gate: None,
            sandbox: SandboxConfig::default(),
            git_remote: None,
            git_branch: "main".to_string(),
        }
    }
}

fn default_schema_version() -> u32 { 1 }
fn default_env() -> String { "default".to_string() }
fn default_git_branch() -> String { "main".to_string() }

// ─── infisical profiles ───────────────────────────────────────────────────────

/// A single named Infisical account's connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfisicalConfig {
    #[serde(default = "default_infisical_url")]
    pub site_url: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    /// Infisical project-scoped service token (st.xxx) — primary auth method.
    /// When set, used directly as a Bearer token; no exchange needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_token: Option<String>,
    #[serde(default)]
    pub default_project_id: Option<String>,
    #[serde(default = "default_infisical_env")]
    pub default_environment: String,
}

impl InfisicalConfig {
    /// Returns true if enough credentials are present to attempt an API call.
    pub fn is_configured(&self) -> bool {
        self.service_token.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false)
            || (self.client_id.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
                && self.client_secret.as_ref().map(|s| !s.is_empty()).unwrap_or(false))
    }
}

fn default_infisical_url() -> String {
    "https://app.infisical.com".to_string()
}

fn default_infisical_env() -> String {
    "dev".to_string()
}

/// Resolver over a set of named Infisical profiles.
/// Passed to execution / credential resolution so individual bindings can
/// reference a specific profile by name (or fall back to the active one).
pub struct InfisicalProfiles<'a> {
    pub profiles: &'a BTreeMap<String, InfisicalConfig>,
    pub active: Option<&'a str>,
}

impl<'a> InfisicalProfiles<'a> {
    /// Resolve a specific profile by name, or the active profile if `name` is None.
    pub fn resolve(&self, name: Option<&str>) -> Option<&'a InfisicalConfig> {
        let key = name.or(self.active)?;
        self.profiles.get(key)
    }

    /// Return the active profile, if any.
    pub fn active_profile(&self) -> Option<&'a InfisicalConfig> {
        self.resolve(None)
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

// ─── other config types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalMode {
    Auto,
    #[default]
    Interactive,
    AlwaysRequire,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_executables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalGateConfig {
    pub webhook_url: String,
    #[serde(default)]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_from_home() {
        let state = ClixState::from_home(PathBuf::from("/tmp/test-clix"));
        assert_eq!(state.config_path, PathBuf::from("/tmp/test-clix/config.yaml"));
    }

    #[test]
    fn test_default_config() {
        let cfg = ClixConfig::default();
        assert_eq!(cfg.schema_version, 1);
        assert!(matches!(cfg.approval_mode, ApprovalMode::Interactive));
        assert!(cfg.infisical_profiles.is_empty());
        assert!(cfg.active_infisical.is_none());
    }

    #[test]
    fn test_legacy_migration() {
        let mut config = ClixConfig {
            infisical: Some(InfisicalConfig {
                site_url: "https://example.com".to_string(),
                client_id: Some("cid".to_string()),
                client_secret: None,
                service_token: None,
                default_project_id: Some("proj1".to_string()),
                default_environment: "dev".to_string(),
            }),
            ..ClixConfig::default()
        };
        migrate_infisical_config(&mut config);
        assert!(config.infisical.is_none(), "legacy field should be cleared");
        assert!(config.infisical_profiles.contains_key("default"), "promoted to 'default' profile");
        assert_eq!(config.active_infisical.as_deref(), Some("default"));
        let profile = &config.infisical_profiles["default"];
        assert_eq!(profile.site_url, "https://example.com");
        assert_eq!(profile.client_id.as_deref(), Some("cid"));
    }

    #[test]
    fn test_infisical_profiles_resolve() {
        let mut profiles = BTreeMap::new();
        profiles.insert("work".to_string(), InfisicalConfig {
            site_url: "https://work.infisical.com".to_string(),
            client_id: None, client_secret: None, service_token: None,
            default_project_id: None,
            default_environment: "prod".to_string(),
        });
        profiles.insert("personal".to_string(), InfisicalConfig {
            site_url: "https://app.infisical.com".to_string(),
            client_id: None, client_secret: None, service_token: None,
            default_project_id: None,
            default_environment: "dev".to_string(),
        });
        let resolver = InfisicalProfiles { profiles: &profiles, active: Some("work") };
        assert_eq!(resolver.active_profile().map(|p| p.default_environment.as_str()), Some("prod"));
        assert_eq!(resolver.resolve(Some("personal")).map(|p| p.default_environment.as_str()), Some("dev"));
        assert!(resolver.resolve(Some("nonexistent")).is_none());
    }
}
