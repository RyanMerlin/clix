pub mod redact;
pub use redact::SecretRedactor;

use std::collections::HashMap;
use crate::error::{ClixError, Result};
use crate::manifest::capability::CredentialSource;
use crate::manifest::profile::ProfileSecretBinding;
use crate::state::InfisicalConfig;

/// Resolve credential sources to a map of env-var-name → value.
/// `profile_bindings` override same-named entries from `creds` (profile wins over capability defaults).
pub fn resolve_credentials(
    creds: &[CredentialSource],
    infisical_cfg: Option<&InfisicalConfig>,
    profile_bindings: &[ProfileSecretBinding],
) -> Result<HashMap<String, String>> {
    // Seed from capability-declared credentials
    let mut effective: HashMap<String, &CredentialSource> = creds.iter()
        .map(|c| (inject_as_of(c).to_string(), c))
        .collect();
    // Profile bindings override capability defaults
    let profile_sources: Vec<&CredentialSource> = profile_bindings.iter().map(|b| &b.source).collect();
    for binding in profile_bindings {
        effective.insert(binding.inject_as.clone(), profile_sources[profile_bindings.iter().position(|b| b.inject_as == binding.inject_as).unwrap()]);
    }

    let mut resolved = HashMap::new();
    for (key, cred) in &effective {
        let value = match *cred {
            CredentialSource::Literal { value, .. } => value.clone(),
            CredentialSource::Env { env_var, .. } => std::env::var(env_var).unwrap_or_default(),
            CredentialSource::Infisical { secret_ref, .. } => {
                let cfg = infisical_cfg.ok_or_else(|| ClixError::CredentialResolution("Infisical requires config".to_string()))?;
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
    let client = reqwest::blocking::Client::new();
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
    let client = reqwest::blocking::Client::new();
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
    let client = reqwest::blocking::Client::new();
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
    let client_id = cfg.client_id.clone()
        .or_else(|| std::env::var("INFISICAL_UNIVERSAL_AUTH_CLIENT_ID").ok())
        .unwrap_or_default();
    let client_secret = cfg.client_secret.clone()
        .or_else(|| std::env::var("INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET").ok())
        .unwrap_or_default();
    let url = format!("{}/api/v1/auth/universal-auth/login", cfg.site_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::new();
    let resp = client.post(&url).json(&serde_json::json!({"clientId": client_id, "clientSecret": client_secret}))
        .send().map_err(|e| ClixError::CredentialResolution(format!("Infisical auth: {e}")))?;
    let body: serde_json::Value = resp.json().map_err(|e| ClixError::CredentialResolution(e.to_string()))?;
    body["accessToken"].as_str().map(|s| s.to_string())
        .ok_or_else(|| ClixError::CredentialResolution("accessToken missing".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::CredentialSource;

    #[test]
    fn test_resolve_env() {
        std::env::set_var("CLIX_TEST_SECRET_VAR", "env-value-123");
        let creds = vec![CredentialSource::Env { env_var: "CLIX_TEST_SECRET_VAR".to_string(), inject_as: "TARGET".to_string() }];
        let resolved = resolve_credentials(&creds, None, &[]).unwrap();
        assert_eq!(resolved.get("TARGET").unwrap(), "env-value-123");
        std::env::remove_var("CLIX_TEST_SECRET_VAR");
    }

    #[test]
    fn test_resolve_literal() {
        let creds = vec![CredentialSource::Literal { value: "lit-val".to_string(), inject_as: "INJECTED".to_string() }];
        let resolved = resolve_credentials(&creds, None, &[]).unwrap();
        assert_eq!(resolved.get("INJECTED").unwrap(), "lit-val");
    }

    #[test]
    fn test_profile_binding_overrides_capability() {
        use crate::manifest::profile::ProfileSecretBinding;
        // Capability declares literal "cap-default"
        let creds = vec![CredentialSource::Literal { value: "cap-default".to_string(), inject_as: "MY_TOKEN".to_string() }];
        // Profile overrides with literal "profile-override"
        let bindings = vec![ProfileSecretBinding {
            inject_as: "MY_TOKEN".to_string(),
            source: CredentialSource::Literal { value: "profile-override".to_string(), inject_as: "MY_TOKEN".to_string() },
        }];
        let resolved = resolve_credentials(&creds, None, &bindings).unwrap();
        assert_eq!(resolved.get("MY_TOKEN").unwrap(), "profile-override");
    }
}
