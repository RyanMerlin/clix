use crate::error::{ClixError, Result};

pub fn validate_input(schema: &serde_json::Value, input: &serde_json::Value) -> Result<()> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|e| ClixError::Schema(format!("invalid schema: {e}")))?;
    let errors: Vec<String> = validator.iter_errors(input).map(|e| e.to_string()).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ClixError::InputValidation(errors.join("; ")))
    }
}

/// Fast-path boolean check — avoids collecting error strings when the caller
/// only needs a pass/fail decision (e.g. pre-execution input gate).
pub fn input_is_valid(schema: &serde_json::Value, input: &serde_json::Value) -> bool {
    jsonschema::validator_for(schema)
        .map(|v| v.is_valid(input))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_input_passes() {
        let schema = serde_json::json!({"type":"object","properties":{"ns":{"type":"string"}},"required":["ns"]});
        assert!(validate_input(&schema, &serde_json::json!({"ns":"default"})).is_ok());
    }

    #[test]
    fn test_missing_required_fails() {
        let schema = serde_json::json!({"type":"object","properties":{"ns":{"type":"string"}},"required":["ns"]});
        assert!(validate_input(&schema, &serde_json::json!({})).is_err());
    }

    #[test]
    fn test_wrong_type_fails() {
        let schema = serde_json::json!({"type":"object","properties":{"count":{"type":"integer"}},"required":["count"]});
        assert!(validate_input(&schema, &serde_json::json!({"count":"not-a-number"})).is_err());
    }

    #[test]
    fn test_is_valid_fast_path() {
        let schema = serde_json::json!({"type":"object","properties":{"x":{"type":"integer"}},"required":["x"]});
        assert!(input_is_valid(&schema, &serde_json::json!({"x": 1})));
        assert!(!input_is_valid(&schema, &serde_json::json!({})));
    }
}
