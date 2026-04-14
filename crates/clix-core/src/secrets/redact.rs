use std::collections::HashMap;

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
}
