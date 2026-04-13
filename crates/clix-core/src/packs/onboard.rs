use std::path::Path;
use serde::Serialize;
use chrono::{DateTime, Utc};
use crate::error::Result;
use crate::manifest::capability::CapabilityManifest;
use super::scaffold::{scaffold_pack, Preset};

#[derive(Debug, Serialize)]
pub struct OnboardReport {
    pub cli: String,
    pub probed_at: DateTime<Utc>,
    pub version_output: Option<String>,
    pub help_sections: Vec<String>,
    pub inferred_subcommands: Vec<String>,
    pub suggested_preset: String,
    pub confidence: f32,
    pub suggested_capabilities: Vec<CapabilityManifest>,
    pub warnings: Vec<String>,
    pub scaffold_path: Option<std::path::PathBuf>,
}

/// Probe a CLI binary and generate a pack scaffold + OnboardReport.
pub fn onboard_cli(
    pack_name: &str,
    command: &str,
    out_dir: &Path,
) -> Result<OnboardReport> {
    let probed_at = Utc::now();
    let mut warnings = vec![];

    let version_output = probe_command(command, &["--version"])
        .or_else(|_| probe_command(command, &["version"]))
        .ok();

    let help_output = probe_command(command, &["--help"])
        .or_else(|_| probe_command(command, &["help"]))
        .unwrap_or_default();

    let subcommands = infer_subcommands(&help_output);
    let (preset, confidence) = infer_preset(&help_output, &subcommands);

    let scaffold_path = scaffold_pack(pack_name, preset.clone(), Some(command), out_dir).ok();
    if scaffold_path.is_none() {
        warnings.push("failed to generate scaffold".to_string());
    }

    Ok(OnboardReport {
        cli: command.to_string(),
        probed_at,
        version_output,
        help_sections: vec![help_output],
        inferred_subcommands: subcommands,
        suggested_preset: format!("{preset:?}").to_lowercase(),
        confidence,
        suggested_capabilities: vec![],
        warnings,
        scaffold_path,
    })
}

fn probe_command(command: &str, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .map_err(|e| crate::error::ClixError::Backend(e.to_string()))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

fn infer_subcommands(help: &str) -> Vec<String> {
    let mut cmds = vec![];
    for line in help.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(|c: char| c.is_lowercase()) && trimmed.contains("  ") {
            let word = trimmed.split_whitespace().next().unwrap_or("");
            if !word.is_empty() && word.len() < 20 && !word.starts_with('-') {
                cmds.push(word.to_string());
            }
        }
    }
    cmds.dedup();
    cmds
}

fn infer_preset(help: &str, subcommands: &[String]) -> (Preset, f32) {
    let lower = help.to_lowercase();
    let has_destructive = subcommands.iter().any(|s| matches!(s.as_str(), "apply" | "delete" | "destroy" | "rm" | "remove"))
        || lower.contains("apply") || lower.contains("destroy") || lower.contains("delete");
    let has_operator = lower.contains("reconcile") || lower.contains("sync") || lower.contains("deploy");

    if has_destructive {
        (Preset::ChangeControlled, 0.7)
    } else if has_operator {
        (Preset::Operator, 0.65)
    } else {
        (Preset::ReadOnly, 0.8)
    }
}
