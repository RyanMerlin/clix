use std::collections::HashMap;
use std::path::PathBuf;
use crate::error::{ClixError, Result};

#[derive(Debug, Clone)]
pub struct SubprocessResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_subprocess(command: &str, args: &[String], cwd: &PathBuf, secrets: &HashMap<String, String>) -> Result<SubprocessResult> {
    let mut cmd = std::process::Command::new(command);
    cmd.args(args).current_dir(cwd);
    let mut env: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in secrets { env.insert(k.clone(), v.clone()); }
    cmd.env_clear();
    for (k, v) in &env { cmd.env(k, v); }
    let output = cmd.output()
        .or_else(|e| {
            // On Windows, many CLIs ship as .cmd wrappers; retry with that extension.
            #[cfg(target_os = "windows")]
            if e.kind() == std::io::ErrorKind::NotFound && !command.ends_with(".cmd") {
                let cmd_name = format!("{command}.cmd");
                let mut cmd2 = std::process::Command::new(&cmd_name);
                cmd2.args(args).current_dir(cwd);
                cmd2.env_clear();
                for (k, v) in &env { cmd2.env(k, v); }
                return cmd2.output().map_err(|_| e);
            }
            Err(e)
        })
        .map_err(|e| ClixError::Backend(format!("failed to spawn `{command}`: {e}")))?;
    Ok(SubprocessResult {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub fn expand_secret_refs(args: &[String], secrets: &HashMap<String, String>) -> Vec<String> {
    args.iter().map(|arg| os_str_expand(arg, |key| {
        secrets.get(key).cloned().or_else(|| std::env::var(key).ok()).unwrap_or_default()
    })).collect()
}

fn os_str_expand(s: &str, lookup: impl Fn(&str) -> String) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let (key, _braced) = if chars.peek() == Some(&'{') {
                chars.next();
                let key: String = chars.by_ref().take_while(|&c| c != '}').collect();
                (key, true)
            } else {
                let key: String = std::iter::from_fn(|| chars.next_if(|c| c.is_alphanumeric() || *c == '_')).collect();
                (key, false)
            };
            result.push_str(&lookup(&key));
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_secret_refs() {
        let secrets = HashMap::from([("TOKEN".to_string(), "abc123".to_string())]);
        let args = vec!["Bearer $TOKEN".to_string(), "${TOKEN}".to_string()];
        let expanded = expand_secret_refs(&args, &secrets);
        assert_eq!(expanded[0], "Bearer abc123");
        assert_eq!(expanded[1], "abc123");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_run_echo() {
        let result = run_subprocess("echo", &["hello".to_string()], &PathBuf::from("."), &HashMap::new()).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
    }
}
