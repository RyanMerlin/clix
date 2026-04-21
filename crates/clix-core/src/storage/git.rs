/// Git-backed sync for the clix home directory.
///
/// Reads and writes still use `FsStorage` (local disk is the source of truth);
/// this module adds `init`, `pull`, `push`, and `status` operations that
/// synchronise the local `~/.clix` directory with a remote git repository.
///
/// Authentication is delegated to the system git credential manager (SSH agent,
/// macOS Keychain, git-credential-manager, etc.) — clix does not handle tokens.
use std::io;
use std::path::Path;
use std::process::{Command, Output};

// ── helpers ───────────────────────────────────────────────────────────────────

fn git(args: &[&str], cwd: &Path) -> io::Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
}

fn git_ok(args: &[&str], cwd: &Path) -> io::Result<String> {
    let out = git(args, cwd)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim_end().to_string();
        Err(io::Error::new(io::ErrorKind::Other, stderr))
    }
}

fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

// ── .gitignore ────────────────────────────────────────────────────────────────

const GITIGNORE: &str = "\
# clix auto-generated
receipts.db
receipts.db-*
cache/
*.sock
*.pid
pack-signing.pem
";

fn ensure_gitignore(dir: &Path) -> io::Result<()> {
    let path = dir.join(".gitignore");
    if !path.exists() {
        std::fs::write(&path, GITIGNORE)?;
    }
    Ok(())
}

// ── public API ────────────────────────────────────────────────────────────────

/// Initialise `dir` as a git repository backed by `remote_url`.
///
/// Behaviour:
/// - If `dir` is already a git repo: set/update the `origin` remote, then
///   pull (fast-forward only).
/// - If `dir` is not a git repo: `git init`, create `.gitignore`, commit any
///   existing files, add `origin`, then attempt to pull; if the remote is
///   empty (no commits) push instead.
pub fn init(dir: &Path, remote_url: &str, branch: &str) -> io::Result<()> {
    if is_git_repo(dir) {
        // Update remote URL
        let current_remote = git(&["remote", "get-url", "origin"], dir);
        if current_remote.is_ok() {
            git_ok(&["remote", "set-url", "origin", remote_url], dir)?;
        } else {
            git_ok(&["remote", "add", "origin", remote_url], dir)?;
        }
        // Fetch + pull if there's a remote branch
        let _ = git_ok(&["fetch", "origin"], dir);
        let remote_ref = format!("origin/{branch}");
        if git(&["rev-parse", "--verify", &remote_ref], dir).map(|o| o.status.success()).unwrap_or(false) {
            git_ok(&["pull", "--rebase", "--allow-unrelated-histories", "origin", branch], dir)?;
        }
    } else {
        git_ok(&["init", "-b", branch], dir)?;
        git_ok(&["config", "user.email", "clix@localhost"], dir)?;
        git_ok(&["config", "user.name", "clix"], dir)?;
        ensure_gitignore(dir)?;
        git_ok(&["add", "-A"], dir)?;
        // Initial commit (might be empty if dir is empty)
        let _ = git_ok(&["commit", "--allow-empty", "-m", "chore: clix init"], dir);
        git_ok(&["remote", "add", "origin", remote_url], dir)?;
        // Try to pull remote history; if the remote is empty, push instead
        let fetch = git_ok(&["fetch", "origin"], dir);
        if fetch.is_ok() {
            let remote_ref = format!("origin/{branch}");
            let has_remote = git(&["rev-parse", "--verify", &remote_ref], dir)
                .map(|o| o.status.success())
                .unwrap_or(false);
            if has_remote {
                git_ok(&["pull", "--rebase", "--allow-unrelated-histories", "origin", branch], dir)?;
                git_ok(&["branch", "--set-upstream-to", &format!("origin/{branch}"), branch], dir)?;
            } else {
                // Remote exists but has no commits yet → push our init commit
                git_ok(&["push", "-u", "origin", branch], dir)?;
            }
        }
    }
    Ok(())
}

/// Stage all changes, commit, and push to origin.
pub fn push(dir: &Path, branch: &str) -> io::Result<String> {
    ensure_gitignore(dir)?;
    git_ok(&["add", "-A"], dir)?;

    // Only commit if there's something staged
    let diff = git_ok(&["diff", "--cached", "--name-only"], dir)?;
    let committed = if diff.is_empty() {
        false
    } else {
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        git_ok(&["commit", "-m", &format!("chore: clix sync {timestamp}")], dir)?;
        true
    };

    // Pull before push to fast-forward if remote has changes
    let _ = git_ok(&["pull", "--rebase", "origin", branch], dir);

    git_ok(&["push", "origin", branch], dir)?;

    Ok(if committed {
        format!("Committed and pushed to origin/{branch}")
    } else {
        format!("Nothing to commit — pushed to origin/{branch}")
    })
}

/// Pull from origin (rebase).
pub fn pull(dir: &Path, branch: &str) -> io::Result<String> {
    git_ok(&["fetch", "origin"], dir)?;
    let result = git_ok(&["pull", "--rebase", "origin", branch], dir)?;
    Ok(result)
}

/// Return a human-readable status summary.
pub fn status(dir: &Path) -> io::Result<String> {
    if !is_git_repo(dir) {
        return Ok("Not a git repository — run `clix sync init <url>` to set up".to_string());
    }
    let mut parts = Vec::new();

    // Remote URL
    if let Ok(url) = git_ok(&["remote", "get-url", "origin"], dir) {
        parts.push(format!("remote: {url}"));
    }

    // Current branch + upstream tracking
    if let Ok(branch) = git_ok(&["rev-parse", "--abbrev-ref", "HEAD"], dir) {
        if let Ok(upstream) = git_ok(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"], dir) {
            // commits ahead/behind
            if let Ok(counts) = git_ok(&["rev-list", "--left-right", "--count", &format!("{upstream}...HEAD")], dir) {
                let mut iter = counts.split_whitespace();
                let behind = iter.next().unwrap_or("?");
                let ahead  = iter.next().unwrap_or("?");
                parts.push(format!("branch: {branch} — {ahead} ahead, {behind} behind {upstream}"));
            } else {
                parts.push(format!("branch: {branch} → {upstream}"));
            }
        } else {
            parts.push(format!("branch: {branch} (no upstream)"));
        }
    }

    // Local dirty files
    if let Ok(dirty) = git_ok(&["status", "--short"], dir) {
        if dirty.is_empty() {
            parts.push("working tree: clean".to_string());
        } else {
            let count = dirty.lines().count();
            parts.push(format!("working tree: {count} changed file(s)"));
            parts.push(dirty);
        }
    }

    Ok(parts.join("\n"))
}
