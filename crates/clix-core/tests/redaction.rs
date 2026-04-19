//! Secret redaction tests.
//!
//! Verifies that `SecretRedactor` masks secret values correctly in various contexts.

use std::collections::HashMap;
use clix_core::secrets::redact::SecretRedactor;

fn make_redactor(pairs: &[(&str, &str)]) -> SecretRedactor {
    let map: HashMap<String, String> = pairs.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    SecretRedactor::new(map)
}

/// A known secret value is replaced in a string.
#[test]
fn test_redact_replaces_secret() {
    let r = make_redactor(&[("TOKEN", "super-secret-token")]);
    let output = r.redact("Bearer super-secret-token in header");
    assert!(!output.contains("super-secret-token"), "secret must not appear in output");
    assert!(output.contains("[REDACTED]"), "placeholder should appear");
}

/// Non-secret content is left unchanged.
#[test]
fn test_redact_leaves_non_secret_unchanged() {
    let r = make_redactor(&[("TOKEN", "mysecret")]);
    let output = r.redact("no secrets here");
    assert_eq!(output, "no secrets here");
}

/// Multiple secrets are all redacted.
#[test]
fn test_redact_multiple_secrets() {
    let r = make_redactor(&[("A", "token-a"), ("B", "token-b")]);
    let output = r.redact("token-a and token-b are both secrets");
    assert!(!output.contains("token-a"), "first secret must be redacted");
    assert!(!output.contains("token-b"), "second secret must be redacted");
}

/// Empty secret map leaves content unchanged.
#[test]
fn test_redact_no_secrets() {
    let r = SecretRedactor::new(HashMap::new());
    assert_eq!(r.redact("no secrets configured"), "no secrets configured");
}

/// Secret appearing multiple times is fully redacted.
#[test]
fn test_redact_repeated_secret() {
    let r = make_redactor(&[("KEY", "abc123")]);
    let output = r.redact("key=abc123 foo key=abc123 bar");
    assert!(!output.contains("abc123"), "all occurrences must be redacted");
}

/// Longer secret shadows shorter prefix match (longest-first ordering).
#[test]
fn test_redact_longest_match_first() {
    // "abcdef" is longer than "abc" — the full string should be redacted, not partially.
    let r = make_redactor(&[("SHORT", "abc"), ("LONG", "abcdef")]);
    let output = r.redact("value: abcdef");
    // "abcdef" should be fully matched as [REDACTED], not abc[REDACTED]
    assert_eq!(output, "value: [REDACTED]");
}

/// Empty secret values are excluded from redaction.
#[test]
fn test_empty_secret_excluded() {
    let r = make_redactor(&[("EMPTY", "")]);
    let output = r.redact("value:  trailing");
    assert_eq!(output, "value:  trailing", "empty secret should not cause spurious redaction");
}
