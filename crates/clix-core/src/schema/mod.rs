use crate::error::{ClixError, Result};

pub fn validate_input(schema: &serde_json::Value, input: &serde_json::Value) -> Result<()> {
    let compiled = jsonschema::JSONSchema::compile(schema)
        .map_err(|e| ClixError::Schema(format!("invalid schema: {e}")))?;
    if let Err(errors) = compiled.validate(input) {
        let messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        return Err(ClixError::InputValidation(messages.join("; ")));
    }
    Ok(())
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
}
