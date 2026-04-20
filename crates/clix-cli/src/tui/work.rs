use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

pub type JobId = u64;

pub fn next_job_id() -> JobId {
    JOB_COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub enum WorkRequest {
    TestInfisical {
        cfg: clix_core::state::InfisicalConfig,
        job_id: JobId,
    },
    PingConnectivity {
        cfg: clix_core::state::InfisicalConfig,
        job_id: JobId,
    },
}

pub enum WorkResult {
    InfisicalTested {
        job_id: JobId,
        ok: bool,
        latency_ms: u64,
        keyring_used: bool,
        error: Option<String>,
    },
    ConnectivityPinged {
        job_id: JobId,
        ok: bool,
        latency_ms: u64,
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
            WorkRequest::PingConnectivity { cfg, job_id } => {
                let report = clix_core::secrets::test_connectivity(&cfg);
                let _ = tx.send(WorkResult::ConnectivityPinged {
                    job_id,
                    ok: report.auth_ok,
                    latency_ms: report.latency_ms,
                    error: report.error,
                });
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
        }
    }
}
