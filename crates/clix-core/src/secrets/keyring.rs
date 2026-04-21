// Linux-only keyring integration using the `keyring` crate (libsecret / D-Bus secret-service)
// Falls back silently if no keyring daemon is available (common in WSL without a running agent).

const SERVICE: &str = "clix";

fn key_client_id(profile_name: &str) -> String {
    format!("infisical-client-id:{profile_name}")
}

fn key_client_secret(profile_name: &str) -> String {
    format!("infisical-client-secret:{profile_name}")
}

fn key_service_token(profile_name: &str) -> String {
    format!("infisical-service-token:{profile_name}")
}

pub fn store_service_token(profile_name: &str, token: &str) -> KeyringResult {
    let key = key_service_token(profile_name);
    let entry = match keyring::Entry::new(SERVICE, &key) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    match entry.set_password(token) {
        Ok(()) => KeyringResult::Ok,
        Err(e) => KeyringResult::Unavailable(e.to_string()),
    }
}

pub fn load_service_token(profile_name: &str) -> Option<String> {
    let key = key_service_token(profile_name);
    let entry = keyring::Entry::new(SERVICE, &key).ok()?;
    entry.get_password().ok()
}

pub fn delete_service_token(profile_name: &str) -> KeyringResult {
    let key = key_service_token(profile_name);
    let entry = match keyring::Entry::new(SERVICE, &key) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    let _ = entry.delete_credential();
    KeyringResult::Ok
}

pub enum KeyringResult {
    Ok,
    Unavailable(String),
}

pub fn store_credentials(profile_name: &str, client_id: &str, client_secret: &str) -> KeyringResult {
    let id_key = key_client_id(profile_name);
    let id_entry = match keyring::Entry::new(SERVICE, &id_key) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    if let Err(e) = id_entry.set_password(client_id) {
        return KeyringResult::Unavailable(e.to_string());
    }
    let secret_key = key_client_secret(profile_name);
    let secret_entry = match keyring::Entry::new(SERVICE, &secret_key) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    if let Err(e) = secret_entry.set_password(client_secret) {
        return KeyringResult::Unavailable(e.to_string());
    }
    KeyringResult::Ok
}

pub fn load_credentials(profile_name: &str) -> Option<(String, String)> {
    let id_key = key_client_id(profile_name);
    let id_entry = keyring::Entry::new(SERVICE, &id_key).ok()?;
    let client_id = id_entry.get_password().ok()?;
    let secret_key = key_client_secret(profile_name);
    let secret_entry = keyring::Entry::new(SERVICE, &secret_key).ok()?;
    let client_secret = secret_entry.get_password().ok()?;
    Some((client_id, client_secret))
}

/// Attempt to load from the legacy (pre-multi-profile) unsuffixed keyring keys.
/// Used as a migration fallback for the "default" profile on first run after upgrade.
fn load_legacy_credentials() -> Option<(String, String)> {
    let id_entry = keyring::Entry::new(SERVICE, "infisical-client-id").ok()?;
    let client_id = id_entry.get_password().ok()?;
    let secret_entry = keyring::Entry::new(SERVICE, "infisical-client-secret").ok()?;
    let client_secret = secret_entry.get_password().ok()?;
    Some((client_id, client_secret))
}

pub fn delete_credentials(profile_name: &str) -> KeyringResult {
    let id_key = key_client_id(profile_name);
    let id_entry = match keyring::Entry::new(SERVICE, &id_key) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    let _ = id_entry.delete_credential();
    let secret_key = key_client_secret(profile_name);
    let secret_entry = match keyring::Entry::new(SERVICE, &secret_key) {
        Ok(e) => e,
        Err(e) => return KeyringResult::Unavailable(e.to_string()),
    };
    let _ = secret_entry.delete_credential();
    // Also wipe service token slot
    let tok_key = key_service_token(profile_name);
    if let Ok(tok_entry) = keyring::Entry::new(SERVICE, &tok_key) {
        let _ = tok_entry.delete_credential();
    }
    KeyringResult::Ok
}

/// Called during ClixState::load — overlays keyring creds into each named profile.
/// Also handles the legacy single-slot migration: if a "default" profile has no
/// client_id but the old unsuffixed keyring entry exists, use it.
pub fn merge_keyring_into_config(config: &mut crate::state::ClixConfig) {
    for (name, profile) in config.infisical_profiles.iter_mut() {
        if let Some((id, secret)) = load_credentials(name) {
            profile.client_id = Some(id);
            profile.client_secret = Some(secret);
        } else if name == "default" && profile.client_id.is_none() {
            // Legacy migration: try the old unsuffixed keyring slot.
            if let Some((id, secret)) = load_legacy_credentials() {
                profile.client_id = Some(id);
                profile.client_secret = Some(secret);
            }
        }
        // Service token overlay — takes precedence, stored in a separate slot
        if let Some(tok) = load_service_token(name) {
            profile.service_token = Some(tok);
        }
    }
}
