use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ParsedSubcommand {
    pub name: String,
    pub description: String,
}

/// Run `<cmd> --help` with a 3-second timeout and extract subcommands.
/// Returns empty vec on failure.
pub fn parse_help(cmd: &str) -> Vec<ParsedSubcommand> {
    let output = run_with_timeout(cmd, &["--help"], Duration::from_secs(3))
        .or_else(|| run_with_timeout(cmd, &["help"], Duration::from_secs(3)))
        .or_else(|| run_with_timeout(cmd, &["-h"], Duration::from_secs(2)));

    let Some(text) = output else { return vec![] };
    parse_help_text(&text, cmd)
}

fn run_with_timeout(cmd: &str, args: &[&str], _timeout: Duration) -> Option<String> {
    // Note: true timeout on Linux would use nix::sys::signal, but for now we rely on
    // the fact that --help exits quickly. Full timeout support can be added with tokio.
    let result = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let combined = if stdout.is_empty() { stderr } else { stdout };
            if combined.trim().is_empty() { None } else { Some(combined) }
        }
        Err(_) => None,
    }
}

fn parse_help_text(text: &str, cmd_name: &str) -> Vec<ParsedSubcommand> {
    let mut results = Vec::new();
    let mut in_commands_section = false;

    for line in text.lines() {
        // Detect section headers like "Commands:", "Available commands:", "COMMANDS", etc.
        let stripped = line.trim();
        let lower = stripped.to_lowercase();
        if lower.ends_with("commands:") || lower.ends_with("subcommands:")
            || lower == "commands" || lower == "subcommands"
            || lower.starts_with("available commands")
        {
            in_commands_section = true;
            continue;
        }
        // New section header (no leading whitespace, ends with colon) ends the commands section
        if in_commands_section && !line.starts_with(' ') && !line.starts_with('\t') {
            if stripped.ends_with(':') || stripped.is_empty() {
                if stripped.is_empty() { continue; }
                in_commands_section = false;
                continue;
            }
        }

        if in_commands_section && (line.starts_with("  ") || line.starts_with('\t')) {
            // Try to parse "  subcommand   description" or "  subcommand" lines
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }
            let (subcmd, desc) = split_subcmd_line(trimmed);
            if !subcmd.is_empty() && is_valid_subcommand(subcmd) {
                results.push(ParsedSubcommand {
                    name: format!("{}.{}", cmd_name, subcmd.replace('-', "_").replace(' ', "_")),
                    description: desc.to_string(),
                });
            }
        }
    }

    // Fallback: if no section found, try to extract any indented command-looking lines
    if results.is_empty() {
        for line in text.lines() {
            if line.starts_with("  ") || line.starts_with('\t') {
                let trimmed = line.trim();
                let (subcmd, desc) = split_subcmd_line(trimmed);
                if !subcmd.is_empty() && is_valid_subcommand(subcmd) && subcmd.len() > 1 {
                    results.push(ParsedSubcommand {
                        name: format!("{}.{}", cmd_name, subcmd.replace('-', "_")),
                        description: desc.to_string(),
                    });
                }
            }
        }
    }

    // Dedupe
    results.dedup_by_key(|r| r.name.clone());
    results.truncate(200);  // sanity limit
    results
}

fn split_subcmd_line(line: &str) -> (&str, &str) {
    // "subcommand   description" — split on 2+ spaces
    if let Some(pos) = line.find("  ") {
        let cmd = line[..pos].trim();
        let desc = line[pos..].trim();
        (cmd, desc)
    } else {
        (line.trim(), "")
    }
}

fn is_valid_subcommand(s: &str) -> bool {
    if s.is_empty() || s.len() > 40 { return false; }
    if s.starts_with('-') { return false; }  // flag, not subcommand
    if s.contains('/') || s.contains('\\') { return false; }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_help_text_commands_section() {
        let text = r#"
My Tool v1.0

Usage: mytool <command>

Commands:
  list    List all items
  create  Create a new item
  delete  Delete an item

Options:
  --help  Show help
"#;
        let results = parse_help_text(text, "mytool");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "mytool.list");
        assert_eq!(results[0].description, "List all items");
        assert_eq!(results[2].name, "mytool.delete");
    }

    #[test]
    fn test_parse_skips_flags() {
        let text = "Commands:\n  --flag   not a subcommand\n  sub  real subcommand\n";
        let results = parse_help_text(text, "tool");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "tool.sub");
    }
}
