use std::collections::HashMap;

/// Obfuscated preview of a secret value for display purposes.
/// Returns first-2 + bullets + last-4 for values ≥12 chars,
/// last-4 only for 8-11 chars, all bullets for 1-7 chars,
/// "(unset)" for empty.
///
/// NOTE: slices by byte index — safe because API keys are ASCII.
pub fn preview(value: &str) -> String {
    let n = value.chars().count();
    match n {
        0 => "(unset)".into(),
        1..=7 => "•".repeat(n),
        8..=11 => {
            let tail = &value[value.len().saturating_sub(4)..];
            format!("{}{}", "•".repeat(n.saturating_sub(4)), tail)
        }
        _ => {
            let tail = &value[value.len().saturating_sub(4)..];
            let head = &value[..2];
            format!("{}{}{}", head, "•".repeat(n.saturating_sub(6)), tail)
        }
    }
}

pub struct SecretRedactor {
    secrets: Vec<String>,
}

impl SecretRedactor {
    pub fn new(resolved: HashMap<String, String>) -> Self {
        let mut secrets: Vec<String> = resolved.into_values().filter(|v| !v.is_empty()).collect();
        secrets.sort_by(|a, b| b.len().cmp(&a.len()));
        SecretRedactor { secrets }
    }
    pub fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();
        for secret in &self.secrets { result = result.replace(secret.as_str(), "[REDACTED]"); }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redacts_secret_values() {
        let secrets = HashMap::from([("TOKEN".to_string(), "supersecret123".to_string())]);
        let r = SecretRedactor::new(secrets);
        assert_eq!(r.redact("token: supersecret123"), "token: [REDACTED]");
    }

    #[test]
    fn test_empty_passthrough() {
        let r = SecretRedactor::new(HashMap::new());
        assert_eq!(r.redact("no secrets here"), "no secrets here");
    }

    #[test]
    fn test_longest_match_first() {
        let secrets = HashMap::from([("A".to_string(), "abc".to_string()), ("B".to_string(), "abcdef".to_string())]);
        let r = SecretRedactor::new(secrets);
        assert_eq!(r.redact("value: abcdef"), "value: [REDACTED]");
    }

    // preview() tests
    #[test]
    fn test_preview_empty() {
        assert_eq!(preview(""), "(unset)");
    }

    #[test]
    fn test_preview_short_5() {
        // 5 chars → all bullets
        assert_eq!(preview("abcde"), "•••••");
    }

    #[test]
    fn test_preview_mid_9() {
        // 9 chars → 5 bullets + last 4
        let v = "abcdefghi";
        let result = preview(v);
        assert!(result.starts_with("•••••"));
        assert!(result.ends_with("fghi"));
    }

    #[test]
    fn test_preview_16() {
        // 16 chars → first 2 + 10 bullets + last 4
        let v = "abcdefghijklmnop";
        let result = preview(v);
        assert!(result.starts_with("ab"));
        assert!(result.ends_with("mnop"));
    }

    #[test]
    fn test_preview_64() {
        let v = "a".repeat(64);
        let result = preview(&v);
        assert!(result.starts_with("aa"));
        assert!(result.ends_with("aaaa"));
        assert_eq!(result.chars().filter(|c| *c == '•').count(), 58);
    }
}
