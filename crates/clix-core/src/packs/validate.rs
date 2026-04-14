use std::path::Path;
use crate::error::Result;
use super::discover::discover_pack;

#[derive(Debug)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

/// Validate a pack directory: schema correctness, required files.
/// Returns list of errors (empty = valid).
pub fn validate_pack(path: &Path) -> Result<Vec<ValidationError>> {
    let mut errors = vec![];
    match discover_pack(path) {
        Err(e) => errors.push(ValidationError {
            path: path.display().to_string(),
            message: e.to_string(),
        }),
        Ok(report) => {
            if report.capabilities.is_empty() && report.profiles.is_empty() {
                errors.push(ValidationError {
                    path: "pack.yaml".to_string(),
                    message: "pack defines no capabilities and no profiles".to_string(),
                });
            }
            for warning in report.warnings {
                errors.push(ValidationError {
                    path: path.display().to_string(),
                    message: warning,
                });
            }
        }
    }
    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_empty_pack_warns() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pack.yaml"), "name: empty\nversion: 1\n").unwrap();
        let errs = validate_pack(dir.path()).unwrap();
        assert!(!errs.is_empty());
        assert!(errs[0].message.contains("no capabilities"));
    }

    #[test]
    fn test_validate_missing_pack_yaml_errors() {
        let dir = TempDir::new().unwrap();
        let errs = validate_pack(dir.path()).unwrap();
        assert!(!errs.is_empty());
    }
}
