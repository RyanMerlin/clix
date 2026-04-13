use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::error::Result;

pub fn home_dir() -> PathBuf {
    if let Ok(v) = std::env::var("CLIX_HOME") {
        return PathBuf::from(v);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".clix")
}

#[derive(Debug, Clone)]
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
}

impl ClixState {
    pub fn from_home(home: PathBuf) -> Self {
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
            home,
        }
    }

    pub fn load(home: PathBuf) -> Result<Self> {
        let mut state = Self::from_home(home);
        if state.config_path.exists() {
            let content = std::fs::read_to_string(&state.config_path)?;
            state.config = serde_yaml::from_str(&content)?;
        }
        Ok(state)
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
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

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
    #[serde(default)]
    pub infisical: Option<InfisicalConfig>,
    #[serde(default)]
    pub approval_gate: Option<ApprovalGateConfig>,
    #[serde(default)]
    pub sandbox: SandboxConfig,
}

impl Default for ClixConfig {
    fn default() -> Self {
        ClixConfig {
            schema_version: 1,
            approval_mode: ApprovalMode::Interactive,
            default_env: "default".to_string(),
            workspace_root: None,
            active_profiles: vec![],
            infisical: None,
            approval_gate: None,
            sandbox: SandboxConfig::default(),
        }
    }
}

fn default_schema_version() -> u32 { 1 }
fn default_env() -> String { "default".to_string() }

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
pub struct InfisicalConfig {
    #[serde(default = "default_infisical_url")]
    pub site_url: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
}

fn default_infisical_url() -> String {
    "https://app.infisical.com".to_string()
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
    }
}
