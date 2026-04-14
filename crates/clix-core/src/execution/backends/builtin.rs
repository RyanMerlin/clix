use crate::error::{ClixError, Result};

pub fn builtin_handler(name: &str, input: &serde_json::Value) -> Result<serde_json::Value> {
    match name {
        "date" | "system.date" => {
            Ok(serde_json::json!({ "date": chrono::Utc::now().to_rfc3339(), "exitCode": 0 }))
        }
        "echo" | "system.echo" => {
            let message = input["message"].as_str().unwrap_or("").to_string();
            Ok(serde_json::json!({ "output": message, "exitCode": 0 }))
        }
        _ => Err(ClixError::Backend(format!("unknown builtin: {name}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_builtin_date() { assert!(builtin_handler("date", &serde_json::json!({})).unwrap()["date"].as_str().is_some()); }
    #[test]
    fn test_builtin_echo() { assert_eq!(builtin_handler("echo", &serde_json::json!({"message":"hi"})).unwrap()["output"], "hi"); }
    #[test]
    fn test_builtin_unknown() { assert!(builtin_handler("nope", &serde_json::json!({})).is_err()); }
}
