use std::path::{Path, PathBuf};
use crate::error::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum Preset { ReadOnly, ChangeControlled, Operator }

impl std::str::FromStr for Preset {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "read-only" | "readonly"          => Ok(Preset::ReadOnly),
            "change-controlled" | "change"    => Ok(Preset::ChangeControlled),
            "operator"                        => Ok(Preset::Operator),
            _ => Err(format!("unknown preset: {s} (use: read-only, change-controlled, operator)")),
        }
    }
}

/// Generate a minimal pack scaffold in out_dir/<name>/.
pub fn scaffold_pack(name: &str, preset: Preset, command: Option<&str>, out_dir: &Path) -> Result<PathBuf> {
    let pack_dir = out_dir.join(name);
    std::fs::create_dir_all(pack_dir.join("capabilities"))?;
    std::fs::create_dir_all(pack_dir.join("profiles"))?;
    std::fs::create_dir_all(pack_dir.join("workflows"))?;

    let cmd = command.unwrap_or(name);

    std::fs::write(pack_dir.join("pack.yaml"), format!(
        "name: {name}\nversion: 1\ndescription: '{name} pack'\nprofiles:\n  - {name}\n"
    ))?;

    std::fs::write(pack_dir.join("profiles").join(format!("{name}.yaml")), format!(
        "name: {name}\nversion: 1\ncapabilities:\n  - {name}.version\n"
    ))?;

    let (cap_name, cap_content) = match preset {
        Preset::ReadOnly => (
            format!("{name}.version"),
            format!(
                "name: {name}.version\nversion: 1\ndescription: Show {cmd} version\nbackend:\n  type: subprocess\n  command: {cmd}\n  args: [\"--version\"]\nrisk: low\nsideEffectClass: readOnly\ninputSchema:\n  type: object\n  properties: {{}}\n"
            ),
        ),
        Preset::ChangeControlled => (
            format!("{name}.apply"),
            format!(
                "name: {name}.apply\nversion: 1\ndescription: Apply changes with {cmd}\nbackend:\n  type: subprocess\n  command: {cmd}\n  args: [\"apply\", \"-f\", \"{{{{ input.file }}}}\"]\nrisk: high\nsideEffectClass: mutating\napprovalPolicy: require\ninputSchema:\n  type: object\n  properties:\n    file:\n      type: string\n  required: [file]\n"
            ),
        ),
        Preset::Operator => (
            format!("{name}.status"),
            format!(
                "name: {name}.status\nversion: 1\ndescription: Show {cmd} status\nbackend:\n  type: subprocess\n  command: {cmd}\n  args: [\"status\"]\nrisk: low\nsideEffectClass: readOnly\ninputSchema:\n  type: object\n  properties: {{}}\n"
            ),
        ),
    };

    std::fs::write(pack_dir.join("capabilities").join(format!("{cap_name}.yaml")), cap_content)?;

    Ok(pack_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scaffold_readonly() {
        let dir = TempDir::new().unwrap();
        let pack_dir = scaffold_pack("mytool", Preset::ReadOnly, Some("mytool"), dir.path()).unwrap();
        assert!(pack_dir.join("pack.yaml").exists());
        assert!(pack_dir.join("profiles").join("mytool.yaml").exists());
        assert!(pack_dir.join("capabilities").join("mytool.version.yaml").exists());
    }
}
