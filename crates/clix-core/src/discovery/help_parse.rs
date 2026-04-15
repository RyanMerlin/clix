use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct ParsedSubcommand {
    pub name: String,
    pub description: String,
}

/// Parse subcommands from `cmd --help`, with one level of recursion for
/// group commands (e.g. `gh pr` → `gh.pr.list`, `gh.pr.create`, …).
pub fn parse_help(cmd: &str) -> Vec<ParsedSubcommand> {
    let text = run_help(cmd, &[]).unwrap_or_default();
    if text.is_empty() { return vec![]; }

    let top = parse_help_text(&text, cmd);

    // Recurse one level into group commands (those whose description suggests they
    // have subcommands: starts with "Manage", "Work with", "View details about", etc.)
    let mut results = Vec::new();
    for sub in &top {
        let sub_name_part = sub.name.trim_start_matches(&format!("{}.", cmd));
        if is_group_command(&sub.description) {
            // Run `cmd sub --help` and discover its sub-subcommands
            let child_text = run_help(cmd, &[sub_name_part]).unwrap_or_default();
            if !child_text.is_empty() {
                let children = parse_help_text(&child_text, &sub.name);
                if !children.is_empty() {
                    results.extend(children);
                    continue; // replaced parent with children
                }
            }
        }
        results.push(sub.clone());
    }

    results.dedup_by_key(|r| r.name.clone());
    results.truncate(300);
    results
}

fn is_group_command(description: &str) -> bool {
    let lower = description.to_lowercase();
    lower.starts_with("manage ")
        || lower.starts_with("work with ")
        || lower.starts_with("view details")
        || lower.starts_with("create and manage")
        || lower.starts_with("interact with")
}

fn run_help(cmd: &str, args: &[&str]) -> Option<String> {
    // Try --help first, then -h, then help subcommand
    let help_variants: &[&[&str]] = &[
        &[args, &["--help"]].concat(),
        &[args, &["-h"]].concat(),
    ];
    // If args is empty also try bare "help" subcommand
    let help_sub: Vec<&str> = vec!["help"];
    let variants: Vec<Vec<&str>> = if args.is_empty() {
        let mut v: Vec<Vec<&str>> = help_variants.iter().map(|a| a.to_vec()).collect();
        v.push(help_sub);
        v
    } else {
        help_variants.iter().map(|a| a.to_vec()).collect()
    };

    for variant in &variants {
        if let Some(text) = run_cmd(cmd, variant) {
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    None
}

fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    let result = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let text = if stdout.trim().is_empty() { stderr } else { stdout };
            if text.trim().is_empty() { None } else { Some(text) }
        }
        Err(_) => None,
    }
}

fn parse_help_text(text: &str, cmd_name: &str) -> Vec<ParsedSubcommand> {
    let mut results = Vec::new();
    let mut in_commands_section = false;

    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        // ── Section header detection ──────────────────────────────────────────
        // Matches: "Commands:", "COMMANDS", "CORE COMMANDS", "Available Commands:", etc.
        // Strategy: if a line (after trimming) ends with "commands" or "commands:",
        // case-insensitively, treat it as a command section start.
        if lower == "commands"
            || lower.ends_with(" commands")
            || lower.ends_with(" commands:")
            || lower == "commands:"
            || lower.ends_with("subcommands")
            || lower.ends_with("subcommands:")
            || lower.starts_with("available command")
        {
            in_commands_section = true;
            continue;
        }

        // A non-indented non-empty line that doesn't look like a subcommand ends the section
        if in_commands_section && !line.starts_with(' ') && !line.starts_with('\t') {
            if !trimmed.is_empty() {
                in_commands_section = false;
                continue;
            }
        }

        if in_commands_section && (line.starts_with("  ") || line.starts_with('\t')) {
            if trimmed.is_empty() { continue; }
            let (raw_name, desc) = split_subcmd_line(trimmed);
            // Strip trailing colon — gh uses "auth:  desc" format
            let name = raw_name.trim_end_matches(':');
            if !name.is_empty() && is_valid_subcommand(name) {
                let dotted = format!("{}.{}", cmd_name, name.replace('-', "_"));
                results.push(ParsedSubcommand {
                    name: dotted,
                    description: desc.to_string(),
                });
            }
        }
    }

    // Fallback: no section found — try any indented 2-column line
    if results.is_empty() {
        for line in text.lines() {
            if !(line.starts_with("  ") || line.starts_with('\t')) { continue; }
            let trimmed = line.trim();
            let (raw_name, desc) = split_subcmd_line(trimmed);
            let name = raw_name.trim_end_matches(':');
            if !name.is_empty() && is_valid_subcommand(name) && name.len() > 1 {
                let dotted = format!("{}.{}", cmd_name, name.replace('-', "_"));
                results.push(ParsedSubcommand {
                    name: dotted,
                    description: desc.to_string(),
                });
            }
        }
    }

    results.dedup_by_key(|r| r.name.clone());
    results.truncate(200);
    results
}

/// Split "subcmd   description" on the first run of 2+ spaces.
/// Handles both "  auth:  desc" and "  auth    desc" formats.
fn split_subcmd_line(line: &str) -> (&str, &str) {
    // Find position of 2+ consecutive spaces
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            // Count run of spaces
            let start = i;
            while i < bytes.len() && bytes[i] == b' ' { i += 1; }
            if i - start >= 2 {
                return (line[..start].trim(), line[i..].trim());
            }
        } else {
            i += 1;
        }
    }
    (line.trim(), "")
}

fn is_valid_subcommand(s: &str) -> bool {
    if s.is_empty() || s.len() > 40 { return false; }
    if s.starts_with('-') { return false; }  // flag
    if s.contains('/') || s.contains('\\') || s.contains('.') { return false; }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_standard_section() {
        let text = "Usage: mytool\n\nCommands:\n  list    List items\n  create  Create item\n\nOptions:\n  --help\n";
        let r = parse_help_text(text, "mytool");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "mytool.list");
        assert_eq!(r[1].description, "Create item");
    }

    #[test]
    fn test_parse_gh_style_uppercase_no_colon() {
        let text = "Work with GitHub.\n\nCORE COMMANDS\n  auth:     Authenticate\n  pr:       Manage PRs\n\nFLAGS\n  --help\n";
        let r = parse_help_text(text, "gh");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "gh.auth");  // colon stripped
        assert_eq!(r[1].name, "gh.pr");
    }

    #[test]
    fn test_parse_skips_flags() {
        let text = "Commands:\n  --flag   not a subcommand\n  sub  real subcommand\n";
        let r = parse_help_text(text, "tool");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "tool.sub");
    }

    #[test]
    fn test_split_subcmd_line() {
        assert_eq!(split_subcmd_line("auth:     Authenticate"), ("auth:", "Authenticate"));
        assert_eq!(split_subcmd_line("list  List items"), ("list", "List items"));
        assert_eq!(split_subcmd_line("create"), ("create", ""));
    }
}
