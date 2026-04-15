// Linux-only keyring integration using the `keyring` crate (libsecret / D-Bus secret-service)
// Falls back silently if no keyring daemon is available (common in WSL without a running agent).

const SERVICE: &str = "clix";
const KEY_CLIENT_ID: &str = "infisical-client-id";
const KEY_CLIENT_SECRET: &str = "infisical-client-secret";

pub enum KeyringResult {
    Ok,
    Unavailable(String),
}

pub fn store_credentials(client_id: &str, client_secret: &str) -> KeyringResult {
    let id_entry = match keyring::Entry::new(SERVICE, KEY_CLIENT_ID) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    if let Err(e) = id_entry.set_password(client_id) {
        return KeyringResult::Unavailable(e.to_string());
    }
    let secret_entry = match keyring::Entry::new(SERVICE, KEY_CLIENT_SECRET) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    if let Err(e) = secret_entry.set_password(client_secret) {
        return KeyringResult::Unavailable(e.to_string());
    }
    KeyringResult::Ok
}

pub fn load_credentials() -> Option<(String, String)> {
    let id_entry = keyring::Entry::new(SERVICE, KEY_CLIENT_ID).ok()?;
    let client_id = id_entry.get_password().ok()?;
    let secret_entry = keyring::Entry::new(SERVICE, KEY_CLIENT_SECRET).ok()?;
    let client_secret = secret_entry.get_password().ok()?;
    Some((client_id, client_secret))
}

pub fn delete_credentials() -> KeyringResult {
    let id_entry = match keyring::Entry::new(SERVICE, KEY_CLIENT_ID) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    let _ = id_entry.delete_credential();
    let secret_entry = match keyring::Entry::new(SERVICE, KEY_CLIENT_SECRET) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    let _ = secret_entry.delete_credential();
    KeyringResult::Ok
}

/// Called during ClixState::load — overlays keyring creds into config if present.
/// Silently no-ops if keyring unavailable.
pub fn merge_keyring_into_config(config: &mut crate::state::ClixConfig) {
    if let Some((id, secret)) = load_credentials() {
        let cfg = config.infisical.get_or_insert_with(|| crate::state::InfisicalConfig {
            site_url: "https://app.infisical.com".to_string(),
            client_id: None,
            client_secret: None,
            default_project_id: None,
            default_environment: "dev".to_string(),
        });
        cfg.client_id = Some(id);
        cfg.client_secret = Some(secret);
    }
}
