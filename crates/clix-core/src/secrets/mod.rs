pub mod redact;
pub use redact::{SecretRedactor, preview};

#[cfg(target_os = "linux")]
pub mod keyring;

use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::sync::OnceLock;
use std::sync::Mutex;
use crate::error::{ClixError, Result};
use crate::manifest::capability::CredentialSource;
use crate::manifest::profile::{ProfileSecretBinding, ProfileFolderBinding};
use crate::state::{InfisicalConfig, InfisicalProfiles};

// ─── token cache ─────────────────────────────────────────────────────────────
// Keyed on (site_url, client_id) so multiple accounts don't share a slot.

struct CachedToken {
    token: String,
    expires_at: Instant,
}

type TokenKey = (String, String);

static TOKEN_CACHE: OnceLock<Mutex<HashMap<TokenKey, CachedToken>>> = OnceLock::new();

fn token_cache() -> &'static Mutex<HashMap<TokenKey, CachedToken>> {
    TOKEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_infisical_token_cached(cfg: &InfisicalConfig) -> Result<String> {
    let key: TokenKey = (
        cfg.site_url.clone(),
        cfg.client_id.clone().unwrap_or_default(),
    );
    {
        let cache = token_cache().lock().unwrap();
        if let Some(ct) = cache.get(&key) {
            if ct.expires_at > Instant::now() {
                return Ok(ct.token.clone());
            }
        }
    }
    // stale or missing — re-login
    let (token, ttl) = get_infisical_token_with_ttl(cfg)?;
    {
        let mut cache = token_cache().lock().unwrap();
        cache.insert(key, CachedToken {
            token: token.clone(),
            expires_at: Instant::now() + Duration::from_secs(ttl.saturating_sub(60)),
        });
    }
    Ok(token)
}

// ─── connectivity ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConnectivityReport {
    pub auth_ok: bool,
    pub site_reachable: bool,
    pub workspace_reachable: bool,
    pub root_folder_count: usize,
    pub latency_ms: u64,
    pub token_expires_in: Option<u64>,
    pub error: Option<String>,
}

pub fn test_connectivity(cfg: &InfisicalConfig) -> ConnectivityReport {
    let start = std::time::Instant::now();
    match get_infisical_token_with_ttl(cfg) {
        Err(e) => ConnectivityReport {
            auth_ok: false,
            site_reachable: false,
            workspace_reachable: false,
            root_folder_count: 0,
            latency_ms: start.elapsed().as_millis() as u64,
            token_expires_in: None,
            error: Some(e.to_string()),
        },
        Ok((token, ttl)) => {
            // Warm the cache for this profile's credentials.
            {
                let key = (cfg.site_url.clone(), cfg.client_id.clone().unwrap_or_default());
                let mut cache = token_cache().lock().unwrap();
                cache.insert(key, CachedToken {
                    token: token.clone(),
                    expires_at: Instant::now() + Duration::from_secs(ttl.saturating_sub(60)),
                });
            }
            let project_id = cfg.default_project_id.as_deref().unwrap_or("");
            let env = &cfg.default_environment;
            let folders = if !project_id.is_empty() {
                list_infisical_folders(cfg, project_id, env, "/").unwrap_or_default()
            } else {
                vec![]
            };
            ConnectivityReport {
                auth_ok: true,
                site_reachable: true,
                workspace_reachable: !project_id.is_empty(),
                root_folder_count: folders.len(),
                latency_ms: start.elapsed().as_millis() as u64,
                token_expires_in: Some(ttl),
                error: None,
            }
        }
    }
}

// ─── credential resolution ────────────────────────────────────────────────────

/// Resolve credential sources to a map of env-var-name → value.
/// `profile_bindings` override same-named entries from `creds` (profile wins over capability defaults).
/// `folder_bindings` expand entire Infisical folder snapshots; per-key bindings take precedence.
pub fn resolve_credentials(
    creds: &[CredentialSource],
    infisical: &InfisicalProfiles<'_>,
    profile_bindings: &[ProfileSecretBinding],
    folder_bindings: &[ProfileFolderBinding],
) -> Result<HashMap<String, String>> {
    // Seed from capability-declared credentials
    let mut effective: HashMap<String, CredentialSource> = creds.iter()
        .map(|c| (inject_as_of(c).to_string(), c.clone()))
        .collect();

    // Folder bindings expand entire paths; lowest priority (overridden by per-key bindings)
    for fb in folder_bindings {
        let prefix = fb.inject_prefix.as_deref().unwrap_or("");
        for secret_name in &fb.snapshot {
            let inject_as = format!("{}{}", prefix, secret_name);
            effective.entry(inject_as.clone()).or_insert_with(|| CredentialSource::Infisical {
                inject_as,
                secret_ref: crate::manifest::capability::InfisicalRef {
                    secret_name: secret_name.clone(),
                    project_id: Some(fb.project_id.clone()),
                    environment: fb.environment.clone(),
                    secret_path: fb.secret_path.clone(),
                    infisical_profile: fb.infisical_profile.clone(),
                },
            });
        }
    }

    // Profile bindings override all
    for binding in profile_bindings {
        effective.insert(binding.inject_as.clone(), binding.source.clone());
    }

    let mut resolved = HashMap::new();
    for (key, cred) in &effective {
        let value = match cred {
            CredentialSource::Literal { value, .. } => value.clone(),
            CredentialSource::Env { env_var, inject_as } => {
                std::env::var(env_var).map_err(|_| {
                    ClixError::CredentialResolution(format!(
                        "env var `{env_var}` is not set (required to inject as `{inject_as}`)"
                    ))
                })?
            }
            CredentialSource::Infisical { secret_ref, .. } => {
                let cfg = infisical.resolve(secret_ref.infisical_profile.as_deref())
                    .ok_or_else(|| {
                        let profile_name = secret_ref.infisical_profile.as_deref().unwrap_or("<active>");
                        ClixError::CredentialResolution(format!(
                            "Infisical profile '{profile_name}' not configured (run `clix infisical add` to set one up)"
                        ))
                    })?;
                fetch_infisical_secret(cfg, secret_ref)?
            }
        };
        resolved.insert(key.clone(), value);
    }
    Ok(resolved)
}

fn inject_as_of(c: &CredentialSource) -> &str {
    match c {
        CredentialSource::Literal { inject_as, .. } => inject_as,
        CredentialSource::Env { inject_as, .. } => inject_as,
        CredentialSource::Infisical { inject_as, .. } => inject_as,
    }
}

// ─── Infisical API helpers ────────────────────────────────────────────────────

/// List secret names at a given path in Infisical (no values fetched).
pub fn list_infisical_secrets(
    cfg: &InfisicalConfig,
    project_id: &str,
    environment: &str,
    secret_path: &str,
) -> Result<Vec<String>> {
    let token = get_infisical_token(cfg)?;
    let url = format!(
        "{}/api/v3/secrets/raw?workspaceId={}&environment={}&secretPath={}&recursive=false",
        cfg.site_url.trim_end_matches('/'),
        project_id, environment,
        urlencoding::encode(secret_path),
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    let resp = client.get(&url).bearer_auth(&token).send()
        .map_err(|e| ClixError::CredentialResolution(format!("Infisical list secrets: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(ClixError::CredentialResolution(format!("Infisical {status}: {body}")));
    }
    let body: serde_json::Value = resp.json().map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    let names = body["secrets"].as_array()
        .map(|arr| arr.iter().filter_map(|s| s["secretKey"].as_str().map(|n| n.to_string())).collect())
        .unwrap_or_default();
    Ok(names)
}

/// List subfolder names at a given path in Infisical.
pub fn list_infisical_folders(
    cfg: &InfisicalConfig,
    project_id: &str,
    environment: &str,
    secret_path: &str,
) -> Result<Vec<String>> {
    let token = get_infisical_token(cfg)?;
    let url = format!(
        "{}/api/v1/folders?workspaceId={}&environment={}&secretPath={}",
        cfg.site_url.trim_end_matches('/'),
        project_id, environment,
        urlencoding::encode(secret_path),
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    let resp = client.get(&url).bearer_auth(&token).send()
        .map_err(|e| ClixError::CredentialResolution(format!("Infisical list folders: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(ClixError::CredentialResolution(format!("Infisical {status}: {body}")));
    }
    let body: serde_json::Value = resp.json().map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    let names = body["folders"].as_array()
        .map(|arr| arr.iter().filter_map(|f| f["name"].as_str().map(|n| n.to_string())).collect())
        .unwrap_or_default();
    Ok(names)
}

fn fetch_infisical_secret(cfg: &InfisicalConfig, secret_ref: &crate::manifest::capability::InfisicalRef) -> Result<String> {
    let token = get_infisical_token(cfg)?;
    let project_id = secret_ref.project_id.as_deref().unwrap_or("");
    let url = format!(
        "{}/api/v3/secrets/raw/{}?workspaceId={}&environment={}&secretPath={}",
        cfg.site_url.trim_end_matches('/'),
        secret_ref.secret_name, project_id, secret_ref.environment,
        urlencoding::encode(&secret_ref.secret_path),
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    let resp = client.get(&url).bearer_auth(&token).send()
        .map_err(|e| ClixError::CredentialResolution(format!("Infisical HTTP: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(ClixError::CredentialResolution(format!("Infisical {status}: {body}")));
    }
    let body: serde_json::Value = resp.json().map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    body["secret"]["secretValue"].as_str().map(|s| s.to_string())
        .ok_or_else(|| ClixError::CredentialResolution("secretValue missing".to_string()))
}

fn get_infisical_token(cfg: &InfisicalConfig) -> Result<String> {
    get_infisical_token_cached(cfg)
}

fn get_infisical_token_with_ttl(cfg: &InfisicalConfig) -> Result<(String, u64)> {
    let client_id = cfg.client_id.clone()
        .or_else(|| std::env::var("INFISICAL_UNIVERSAL_AUTH_CLIENT_ID").ok())
        .unwrap_or_default();
    let client_secret = cfg.client_secret.clone()
        .or_else(|| std::env::var("INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET").ok())
        .unwrap_or_default();
    let url = format!("{}/api/v1/auth/universal-auth/login", cfg.site_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    let resp = client.post(&url).json(&serde_json::json!({"clientId": client_id, "clientSecret": client_secret}))
        .send().map_err(|e| ClixError::CredentialResolution(format!("Infisical auth: {e}")))?;
    let body: serde_json::Value = resp.json().map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    let token = body["accessToken"].as_str().map(|s| s.to_string())
        .ok_or_else(|| ClixError::CredentialResolution("accessToken missing".to_string()))?;
    let ttl = body["expiresIn"].as_u64().unwrap_or(7200);
    Ok((token, ttl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::CredentialSource;
    use std::collections::BTreeMap;

    fn empty_profiles() -> BTreeMap<String, InfisicalConfig> { BTreeMap::new() }

    #[test]
    fn test_resolve_env() {
        std::env::set_var("CLIX_TEST_SECRET_VAR", "env-value-123");
        let creds = vec![CredentialSource::Env { env_var: "CLIX_TEST_SECRET_VAR".to_string(), inject_as: "TARGET".to_string() }];
        let profiles = empty_profiles();
        let resolver = InfisicalProfiles { profiles: &profiles, active: None };
        let resolved = resolve_credentials(&creds, &resolver, &[], &[]).unwrap();
        assert_eq!(resolved.get("TARGET").unwrap(), "env-value-123");
        std::env::remove_var("CLIX_TEST_SECRET_VAR");
    }

    #[test]
    fn test_resolve_literal() {
        let creds = vec![CredentialSource::Literal { value: "lit-val".to_string(), inject_as: "INJECTED".to_string() }];
        let profiles = empty_profiles();
        let resolver = InfisicalProfiles { profiles: &profiles, active: None };
        let resolved = resolve_credentials(&creds, &resolver, &[], &[]).unwrap();
        assert_eq!(resolved.get("INJECTED").unwrap(), "lit-val");
    }

    #[test]
    fn test_profile_binding_overrides_capability() {
        use crate::manifest::profile::ProfileSecretBinding;
        let creds = vec![CredentialSource::Literal { value: "cap-default".to_string(), inject_as: "MY_TOKEN".to_string() }];
        let bindings = vec![ProfileSecretBinding {
            inject_as: "MY_TOKEN".to_string(),
            source: CredentialSource::Literal { value: "profile-override".to_string(), inject_as: "MY_TOKEN".to_string() },
        }];
        let profiles = empty_profiles();
        let resolver = InfisicalProfiles { profiles: &profiles, active: None };
        let resolved = resolve_credentials(&creds, &resolver, &bindings, &[]).unwrap();
        assert_eq!(resolved.get("MY_TOKEN").unwrap(), "profile-override");
    }

    #[test]
    fn env_credential_missing_var_is_error() {
        let var_name = "CLIX_TEST_VAR_DEFINITELY_NOT_SET_12345";
        std::env::remove_var(var_name);
        let creds = vec![CredentialSource::Env {
            env_var: var_name.to_string(),
            inject_as: "TARGET_VAR".to_string(),
        }];
        let profiles = empty_profiles();
        let resolver = InfisicalProfiles { profiles: &profiles, active: None };
        let err = resolve_credentials(&creds, &resolver, &[], &[]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(var_name), "error should name the missing var: {msg}");
        assert!(msg.contains("TARGET_VAR"), "error should name the inject_as key: {msg}");
    }

    #[test]
    fn env_credential_set_var_resolves() {
        let var_name = "CLIX_TEST_VAR_PRESENT_12345";
        std::env::set_var(var_name, "test-value");
        let creds = vec![CredentialSource::Env {
            env_var: var_name.to_string(),
            inject_as: "TARGET_VAR".to_string(),
        }];
        let profiles = empty_profiles();
        let resolver = InfisicalProfiles { profiles: &profiles, active: None };
        let resolved = resolve_credentials(&creds, &resolver, &[], &[]).unwrap();
        assert_eq!(resolved.get("TARGET_VAR").unwrap(), "test-value");
        std::env::remove_var(var_name);
    }
}
