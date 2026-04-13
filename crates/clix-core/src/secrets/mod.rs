pub mod redact;
pub use redact::SecretRedactor;

use std::collections::HashMap;
use crate::error::{ClixError, Result};
use crate::manifest::capability::CredentialSource;
use crate::state::InfisicalConfig;

pub fn resolve_credentials(creds: &[CredentialSource], infisical_cfg: Option<&InfisicalConfig>) -> Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();
    for cred in creds {
        match cred {
            CredentialSource::Literal { value, inject_as } => { resolved.insert(inject_as.clone(), value.clone()); }
            CredentialSource::Env { env_var, inject_as } => {
                resolved.insert(inject_as.clone(), std::env::var(env_var).unwrap_or_default());
            }
            CredentialSource::Infisical { secret_ref, inject_as } => {
                let cfg = infisical_cfg.ok_or_else(|| ClixError::CredentialResolution("Infisical requires config".to_string()))?;
                let value = fetch_infisical_secret(cfg, secret_ref)?;
                resolved.insert(inject_as.clone(), value);
            }
        }
    }
    Ok(resolved)
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
        let resolved = resolve_credentials(&creds, None).unwrap();
        assert_eq!(resolved.get("TARGET").unwrap(), "env-value-123");
        std::env::remove_var("CLIX_TEST_SECRET_VAR");
    }

    #[test]
    fn test_resolve_literal() {
        let creds = vec![CredentialSource::Literal { value: "lit-val".to_string(), inject_as: "INJECTED".to_string() }];
        let resolved = resolve_credentials(&creds, None).unwrap();
        assert_eq!(resolved.get("INJECTED").unwrap(), "lit-val");
    }
}
