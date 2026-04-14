use crate::manifest::capability::{Validator, ValidatorKind};

pub fn run_validators(validators: &[Validator], input: &serde_json::Value, cwd: &std::path::Path, resolved_args: &[String]) -> Vec<String> {
    let mut errors = vec![];
    for v in validators {
        match v.kind {
            ValidatorKind::RequiredPath => {
                if !cwd.join(&v.path).exists() { errors.push(format!("Required path missing: {}", v.path)); }
            }
            ValidatorKind::DenyArgs => {
                let args_str = resolved_args.join(" ");
                for forbidden in &v.values {
                    if args_str.contains(forbidden.as_str()) { errors.push(format!("Forbidden argument: {forbidden}")); }
                }
            }
            ValidatorKind::RequiredInputKey => {
                if input.get(&v.key).is_none() { errors.push(format!("Input key missing: {}", v.key)); }
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Validator, ValidatorKind};

    #[test]
    fn test_deny_args() {
        let v = vec![Validator { kind: ValidatorKind::DenyArgs, path: String::new(), key: String::new(), values: vec!["--force".to_string()] }];
        let errs = run_validators(&v, &serde_json::json!({}), std::path::Path::new("."), &["kubectl".to_string(), "--force".to_string()]);
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn test_required_input_key() {
        let v = vec![Validator { kind: ValidatorKind::RequiredInputKey, path: String::new(), key: "ns".to_string(), values: vec![] }];
        assert!(!run_validators(&v, &serde_json::json!({}), std::path::Path::new("."), &[]).is_empty());
        assert!(run_validators(&v, &serde_json::json!({"ns":"default"}), std::path::Path::new("."), &[]).is_empty());
    }
}
