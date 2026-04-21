// Tests for profile secret binding resolution and keyring graceful fallback.
use std::collections::BTreeMap;
use clix_core::manifest::capability::CredentialSource;
use clix_core::manifest::profile::ProfileSecretBinding;
use clix_core::secrets::resolve_credentials;
use clix_core::state::InfisicalProfiles;

fn no_infisical<'a>(map: &'a BTreeMap<String, clix_core::state::InfisicalConfig>) -> InfisicalProfiles<'a> {
    InfisicalProfiles { profiles: map, active: None }
}

#[test]
fn profile_binding_overrides_capability_literal() {
    let map = BTreeMap::new();
    let infisical = no_infisical(&map);
    let creds = vec![CredentialSource::Literal {
        value: "cap-default".to_string(),
        inject_as: "MY_TOKEN".to_string(),
    }];
    let bindings = vec![ProfileSecretBinding {
        inject_as: "MY_TOKEN".to_string(),
        source: CredentialSource::Literal {
            value: "profile-override".to_string(),
            inject_as: "MY_TOKEN".to_string(),
        },
    }];
    let resolved = resolve_credentials(&creds, &infisical, &bindings, &[]).unwrap();
    assert_eq!(resolved.get("MY_TOKEN").unwrap(), "profile-override");
}

#[test]
fn profile_binding_env_resolves() {
    let map = BTreeMap::new();
    let infisical = no_infisical(&map);
    std::env::set_var("CLIX_TEST_INTEGRATION_ENV", "env-from-profile");
    let creds = vec![];
    let bindings = vec![ProfileSecretBinding {
        inject_as: "API_KEY".to_string(),
        source: CredentialSource::Env {
            env_var: "CLIX_TEST_INTEGRATION_ENV".to_string(),
            inject_as: "API_KEY".to_string(),
        },
    }];
    let resolved = resolve_credentials(&creds, &infisical, &bindings, &[]).unwrap();
    assert_eq!(resolved.get("API_KEY").unwrap(), "env-from-profile");
    std::env::remove_var("CLIX_TEST_INTEGRATION_ENV");
}

#[test]
fn multiple_bindings_all_resolved() {
    let map = BTreeMap::new();
    let infisical = no_infisical(&map);
    std::env::set_var("CLIX_TEST_MULTI_A", "alpha");
    std::env::set_var("CLIX_TEST_MULTI_B", "beta");
    let creds = vec![];
    let bindings = vec![
        ProfileSecretBinding {
            inject_as: "KEY_A".to_string(),
            source: CredentialSource::Env {
                env_var: "CLIX_TEST_MULTI_A".to_string(),
                inject_as: "KEY_A".to_string(),
            },
        },
        ProfileSecretBinding {
            inject_as: "KEY_B".to_string(),
            source: CredentialSource::Env {
                env_var: "CLIX_TEST_MULTI_B".to_string(),
                inject_as: "KEY_B".to_string(),
            },
        },
    ];
    let resolved = resolve_credentials(&creds, &infisical, &bindings, &[]).unwrap();
    assert_eq!(resolved.get("KEY_A").unwrap(), "alpha");
    assert_eq!(resolved.get("KEY_B").unwrap(), "beta");
    std::env::remove_var("CLIX_TEST_MULTI_A");
    std::env::remove_var("CLIX_TEST_MULTI_B");
}

#[test]
fn capability_cred_with_no_profile_binding_uses_default() {
    let map = BTreeMap::new();
    let infisical = no_infisical(&map);
    std::env::set_var("CLIX_TEST_DEFAULT_VAR", "default-value");
    let creds = vec![CredentialSource::Env {
        env_var: "CLIX_TEST_DEFAULT_VAR".to_string(),
        inject_as: "INJECTED".to_string(),
    }];
    let resolved = resolve_credentials(&creds, &infisical, &[], &[]).unwrap();
    assert_eq!(resolved.get("INJECTED").unwrap(), "default-value");
    std::env::remove_var("CLIX_TEST_DEFAULT_VAR");
}

/// Keyring load should return None gracefully when no daemon is running (e.g. WSL CI).
#[cfg(target_os = "linux")]
#[test]
fn keyring_load_returns_none_when_unavailable() {
    // In CI/WSL without a secret-service daemon this should not panic.
    let result = clix_core::secrets::keyring::load_credentials("default");
    // Either None (unavailable) or Some (if a daemon happens to be running).
    // We just assert it doesn't panic.
    let _ = result;
}
