use anyhow::{bail, Result};
use clix_core::state::{home_dir, ClixState};
use clix_core::storage::git as git_sync;
use crate::cli::SyncCmd;

pub fn run_sync(cmd: SyncCmd) -> Result<()> {
    match cmd {
        SyncCmd::Init { remote, branch } => cmd_init(&remote, &branch),
        SyncCmd::Push => cmd_push(),
        SyncCmd::Pull => cmd_pull(),
        SyncCmd::Status => cmd_status(),
    }
}

fn cmd_init(remote: &str, branch: &str) -> Result<()> {
    let mut state = ClixState::load(home_dir())?;
    let dir = &state.home;

    println!("Initialising git sync in {}", dir.display());
    println!("  remote: {remote}");
    println!("  branch: {branch}");

    git_sync::init(dir, remote, branch)?;

    // Persist the remote + branch into config so future push/pull don't need args
    state.config.git_remote = Some(remote.to_string());
    state.config.git_branch = branch.to_string();
    state.save_config()?;

    println!("Done — run `clix sync push` to upload, `clix sync pull` to download.");
    Ok(())
}

fn cmd_push() -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let remote = state.config.git_remote.as_deref()
        .ok_or_else(|| anyhow::anyhow!("No git remote configured — run `clix sync init <url>` first"))?;
    let branch = &state.config.git_branch;
    println!("Pushing ~/.clix → {remote} ({branch})…");
    let msg = git_sync::push(&state.home, branch)?;
    println!("{msg}");
    Ok(())
}

fn cmd_pull() -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let remote = state.config.git_remote.as_deref()
        .ok_or_else(|| anyhow::anyhow!("No git remote configured — run `clix sync init <url>` first"))?;
    let branch = &state.config.git_branch;
    println!("Pulling {remote} ({branch}) → ~/.clix…");
    let out = git_sync::pull(&state.home, branch)?;
    println!("{out}");
    Ok(())
}

fn cmd_status() -> Result<()> {
    let state = ClixState::load(home_dir())?;
    if state.config.git_remote.is_none() {
        bail!("No git remote configured — run `clix sync init <url>` first");
    }
    let out = git_sync::status(&state.home)?;
    println!("{out}");
    Ok(())
}
