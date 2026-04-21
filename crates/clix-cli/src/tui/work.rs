use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

pub type JobId = u64;

pub fn next_job_id() -> JobId {
    JOB_COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub enum WorkRequest {
    GitPoll { home: std::path::PathBuf },
    GitPush { home: std::path::PathBuf, branch: String },
    GitPull { home: std::path::PathBuf, branch: String },
    TestInfisical {
        cfg: clix_core::state::InfisicalConfig,
        job_id: JobId,
    },
    PingConnectivity {
        cfg: clix_core::state::InfisicalConfig,
    },
    LoadSecretFolders {
        cfg: clix_core::state::InfisicalConfig,
        project_id: String,
        environment: String,
        path: String,
        job_id: JobId,
    },
    LoadSecretNames {
        cfg: clix_core::state::InfisicalConfig,
        project_id: String,
        environment: String,
        path: String,
        job_id: JobId,
    },
    ParseHelp {
        command: String,
        job_id: JobId,
    },
    ApproveReceipt {
        id: uuid::Uuid,
        approver: String,
        job_id: JobId,
    },
}

pub enum WorkResult {
    GitPolled {
        configured: bool,
        dirty: usize,
        ahead: usize,
        behind: usize,
    },
    GitSynced {
        push: bool,
        ok: bool,
        message: String,
    },
    InfisicalTested {
        job_id: JobId,
        ok: bool,
        latency_ms: u64,
        keyring_used: bool,
        error: Option<String>,
    },
    ConnectivityPinged {
        ok: bool,
        latency_ms: u64,
        error: Option<String>,
    },
    SecretFoldersLoaded {
        job_id: JobId,
        folders: Vec<String>,
        error: Option<String>,
    },
    SecretNamesLoaded {
        job_id: JobId,
        names: Vec<String>,
        error: Option<String>,
    },
    HelpParsed {
        job_id: JobId,
        command: String,
        subcmds: Vec<clix_core::discovery::ParsedSubcommand>,
    },
    ReceiptApproved {
        job_id: JobId,
        id: uuid::Uuid,
        ok: bool,
        error: Option<String>,
    },
}

pub struct WorkPool {
    result_tx: Sender<WorkResult>,
    pub result_rx: Receiver<WorkResult>,
}

impl WorkPool {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { result_tx: tx, result_rx: rx }
    }

    pub fn dispatch(&self, req: WorkRequest) {
        let tx = self.result_tx.clone();
        std::thread::spawn(move || match req {
            WorkRequest::GitPoll { home } => {
                use clix_core::storage::git as gs;
                let configured = home.join(".git").exists();
                if !configured {
                    let _ = tx.send(WorkResult::GitPolled { configured: false, dirty: 0, ahead: 0, behind: 0 });
                    return;
                }
                let dirty = std::process::Command::new("git")
                    .args(["status", "--short"])
                    .current_dir(&home)
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
                    .unwrap_or(0);
                // ahead/behind relative to upstream
                let (ahead, behind) = std::process::Command::new("git")
                    .args(["rev-list", "--left-right", "--count", "@{u}...HEAD"])
                    .current_dir(&home)
                    .output()
                    .ok()
                    .and_then(|o| if o.status.success() {
                        let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        let mut parts = s.split_whitespace();
                        let behind = parts.next()?.parse::<usize>().ok()?;
                        let ahead  = parts.next()?.parse::<usize>().ok()?;
                        Some((ahead, behind))
                    } else { None })
                    .unwrap_or((0, 0));
                let _ = gs::status(&home); // warm cache
                let _ = tx.send(WorkResult::GitPolled { configured, dirty, ahead, behind });
            }
            WorkRequest::GitPush { home, branch } => {
                match clix_core::storage::git::push(&home, &branch) {
                    Ok(msg) => { let _ = tx.send(WorkResult::GitSynced { push: true, ok: true, message: msg }); }
                    Err(e) => { let _ = tx.send(WorkResult::GitSynced { push: true, ok: false, message: e.to_string() }); }
                }
            }
            WorkRequest::GitPull { home, branch } => {
                match clix_core::storage::git::pull(&home, &branch) {
                    Ok(msg) => { let _ = tx.send(WorkResult::GitSynced { push: false, ok: true, message: msg }); }
                    Err(e) => { let _ = tx.send(WorkResult::GitSynced { push: false, ok: false, message: e.to_string() }); }
                }
            }
            WorkRequest::TestInfisical { cfg, job_id } => {
                let report = clix_core::secrets::test_connectivity(&cfg);
                let _ = tx.send(WorkResult::InfisicalTested {
                    job_id,
                    ok: report.auth_ok,
                    latency_ms: report.latency_ms,
                    keyring_used: false,
                    error: report.error,
                });
            }
            WorkRequest::PingConnectivity { cfg } => {
                let report = clix_core::secrets::test_connectivity(&cfg);
                let _ = tx.send(WorkResult::ConnectivityPinged {
                    ok: report.auth_ok,
                    latency_ms: report.latency_ms,
                    error: report.error,
                });
            }
            WorkRequest::LoadSecretFolders { cfg, project_id, environment, path, job_id } => {
                match clix_core::secrets::list_infisical_folders(&cfg, &project_id, &environment, &path) {
                    Ok(folders) => {
                        let _ = tx.send(WorkResult::SecretFoldersLoaded { job_id, folders, error: None });
                    }
                    Err(e) => {
                        let _ = tx.send(WorkResult::SecretFoldersLoaded { job_id, folders: vec![], error: Some(e.to_string()) });
                    }
                }
            }
            WorkRequest::LoadSecretNames { cfg, project_id, environment, path, job_id } => {
                match clix_core::secrets::list_infisical_secrets(&cfg, &project_id, &environment, &path) {
                    Ok(names) => {
                        let _ = tx.send(WorkResult::SecretNamesLoaded { job_id, names, error: None });
                    }
                    Err(e) => {
                        let _ = tx.send(WorkResult::SecretNamesLoaded { job_id, names: vec![], error: Some(e.to_string()) });
                    }
                }
            }
            WorkRequest::ParseHelp { command, job_id } => {
                let subcmds = clix_core::discovery::parse_help(&command);
                let _ = tx.send(WorkResult::HelpParsed { job_id, command, subcmds });
            }
            WorkRequest::ApproveReceipt { id, approver, job_id } => {
                use clix_core::execution::broker_client::BrokerClient;
                match BrokerClient::connect() {
                    Ok(mut client) => match client.send_approve(id, approver, None) {
                        Ok(_) => { let _ = tx.send(WorkResult::ReceiptApproved { job_id, id, ok: true, error: None }); }
                        Err(e) => { let _ = tx.send(WorkResult::ReceiptApproved { job_id, id, ok: false, error: Some(e.to_string()) }); }
                    },
                    Err(e) => { let _ = tx.send(WorkResult::ReceiptApproved { job_id, id, ok: false, error: Some(format!("Broker unavailable: {e}")) }); }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_delivers_result() {
        let pool = WorkPool::new();
        let job_id = next_job_id();
        let cfg = clix_core::state::InfisicalConfig {
            site_url: "http://127.0.0.1:19999".to_string(), // unreachable — will fail fast with timeout
            client_id: Some("test".to_string()),
            client_secret: Some("test".to_string()),
            default_project_id: None,
            default_environment: "dev".to_string(),
        };
        pool.dispatch(WorkRequest::TestInfisical { cfg, job_id });
        // Result must arrive within 20 s (reqwest timeout is 10+5)
        let result = pool.result_rx.recv_timeout(std::time::Duration::from_secs(20));
        assert!(result.is_ok(), "no result arrived in time");
        match result.unwrap() {
            WorkResult::InfisicalTested { job_id: jid, ok, .. } => {
                assert_eq!(jid, job_id);
                assert!(!ok, "expected connection failure");
            }
            _ => panic!("unexpected result variant"),
        }
    }

    #[test]
    fn parse_help_dispatches_and_returns() {
        let pool = WorkPool::new();
        let job_id = next_job_id();
        // echo is always present and exits immediately
        pool.dispatch(WorkRequest::ParseHelp { command: "echo".to_string(), job_id });
        let result = pool.result_rx.recv_timeout(std::time::Duration::from_secs(10));
        assert!(result.is_ok(), "no parse_help result arrived in time");
        match result.unwrap() {
            WorkResult::HelpParsed { job_id: jid, command, .. } => {
                assert_eq!(jid, job_id);
                assert_eq!(command, "echo");
            }
            _ => panic!("unexpected result variant"),
        }
    }
}
