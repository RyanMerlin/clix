use ratatui::{prelude::*, widgets::*};
use crate::tui::app::App;
use crate::tui::theme;

pub struct BrokerScreenState {
    pub running: bool,
    pub pid: Option<u32>,
    pub ping_ms: Option<u64>,
    pub socket_path: String,
    pub adopted: Vec<AdoptedCred>,
}

pub struct AdoptedCred {
    pub kind: String,
    pub detail: String,
    pub adopted_at: Option<std::time::SystemTime>,
}

impl BrokerScreenState {
    pub fn probe() -> Self {
        let socket_path = std::env::var("CLIX_BROKER_SOCKET")
            .unwrap_or_else(|_| "/tmp/clix-broker.sock".to_string());

        let (pid, running) = read_pid_file();
        let ping_ms = try_ping(&socket_path);
        let adopted = scan_adopted_creds();

        Self { running: running || ping_ms.is_some(), pid, ping_ms, socket_path, adopted }
    }
}

fn read_pid_file() -> (Option<u32>, bool) {
    let pid_path = creds_dir()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/tmp"))
        .join("broker.pid")
        .to_path_buf();

    if !pid_path.exists() {
        return (None, false);
    }
    let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
    let pid: i32 = pid_str.trim().parse().unwrap_or(0);
    if pid <= 0 {
        return (None, false);
    }
    let alive = process_alive(pid);
    (Some(pid as u32), alive)
}

fn process_alive(pid: i32) -> bool {
    #[cfg(target_os = "linux")]
    {
        let ret = unsafe { libc::kill(pid, 0) };
        ret == 0
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        false
    }
}

fn try_ping(socket_path: &str) -> Option<u64> {
    use std::os::unix::net::UnixStream;
    use std::io::{BufRead, BufReader, Write};
    use std::time::Instant;

    let start = Instant::now();
    let mut stream = UnixStream::connect(socket_path).ok()?;
    stream.write_all(b"{\"type\":\"ping\"}\n").ok()?;
    let reader = BufReader::new(&stream);
    let line = reader.lines().next()?.ok()?;
    if line.contains("pong") {
        Some(start.elapsed().as_millis() as u64)
    } else {
        None
    }
}

fn creds_dir() -> std::path::PathBuf {
    std::env::var("CLIX_BROKER_CREDS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/var/lib"))
                .join("clix")
                .join("broker")
        })
}

fn scan_adopted_creds() -> Vec<AdoptedCred> {
    let creds = creds_dir();
    let mut result = Vec::new();

    // gcloud ADC
    let adc = creds.join("gcloud").join("adc.json");
    if adc.exists() {
        let adopted_at = std::fs::metadata(&adc).ok().and_then(|m| m.modified().ok());
        result.push(AdoptedCred {
            kind: "gcloud:adc".to_string(),
            detail: adc.display().to_string(),
            adopted_at,
        });
    }

    // gcloud SA registry
    let sa_registry = creds.join("gcloud").join("sa_registry.json");
    if sa_registry.exists() {
        if let Ok(text) = std::fs::read_to_string(&sa_registry) {
            if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                for entry in entries {
                    let email = entry["email"].as_str().unwrap_or("?").to_string();
                    let path = entry["path"].as_str().unwrap_or("?").to_string();
                    let kind = format!("gcloud:sa-{}", &path[..path.len().min(8)]);
                    result.push(AdoptedCred {
                        kind,
                        detail: email,
                        adopted_at: None,
                    });
                }
            }
        }
    }

    // kubectl
    let kubeconfig = creds.join("kubectl").join("kubeconfig");
    if kubeconfig.exists() {
        let adopted_at = std::fs::metadata(&kubeconfig).ok().and_then(|m| m.modified().ok());
        result.push(AdoptedCred {
            kind: "kubectl".to_string(),
            detail: kubeconfig.display().to_string(),
            adopted_at,
        });
    }

    // generic
    if let Ok(entries) = std::fs::read_dir(&creds) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_string();
            if name_str == "gcloud" || name_str == "kubectl" { continue; }
            let secret_env = entry.path().join("secret.env");
            if secret_env.exists() {
                let adopted_at = std::fs::metadata(&secret_env).ok().and_then(|m| m.modified().ok());
                result.push(AdoptedCred {
                    kind: format!("generic:{name_str}"),
                    detail: secret_env.display().to_string(),
                    adopted_at,
                });
            }
        }
    }

    result
}

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let state = app.broker_status.as_ref();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(0)])
        .split(area);

    render_status_card(f, state, chunks[0]);
    render_creds_list(f, state, chunks[1]);
}

fn render_status_card(f: &mut Frame, state: Option<&BrokerScreenState>, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Broker Status ", theme::accent_bold()))
        .border_style(theme::border_dim());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(s) = state {
        let status_style = if s.running { theme::ok() } else { theme::danger() };
        let status_text = if s.running { "RUNNING" } else { "STOPPED" };

        let pid_str = s.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".to_string());
        let ping_str = s.ping_ms.map(|ms| format!("{ms}ms")).unwrap_or_else(|| "—".to_string());

        let lines = vec![
            Line::from(vec![
                Span::styled("  status: ", theme::dim()),
                Span::styled(status_text, status_style),
            ]),
            Line::from(vec![
                Span::styled("  pid:    ", theme::dim()),
                Span::styled(pid_str, theme::normal()),
            ]),
            Line::from(vec![
                Span::styled("  ping:   ", theme::dim()),
                Span::styled(ping_str, theme::normal()),
            ]),
            Line::from(vec![
                Span::styled("  socket: ", theme::dim()),
                Span::styled(s.socket_path.clone(), theme::muted()),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    } else {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  press r to probe broker status", theme::muted())),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }
}

fn render_creds_list(f: &mut Frame, state: Option<&BrokerScreenState>, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Adopted Credentials ", theme::accent_bold()))
        .border_style(theme::border_dim());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = if let Some(s) = state {
        if s.adopted.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "  (no credentials adopted) — run `clix init --adopt-creds gcloud`",
                theme::muted(),
            )))]
        } else {
            s.adopted.iter().map(|cred| {
                let age = cred.adopted_at.map(|t| {
                    if let Ok(dur) = std::time::SystemTime::now().duration_since(t) {
                        let secs = dur.as_secs();
                        if secs < 3600 { format!("{}m ago", secs / 60) }
                        else if secs < 86400 { format!("{}h ago", secs / 3600) }
                        else { format!("{}d ago", secs / 86400) }
                    } else { "?".to_string() }
                }).unwrap_or_else(|| "?".to_string());

                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {:30}", cred.kind), theme::accent()),
                    Span::styled(format!("  {:40}", cred.detail), theme::dim()),
                    Span::styled(format!("  {age}"), theme::muted()),
                ]))
            }).collect()
        }
    } else {
        vec![ListItem::new(Line::from(Span::styled("  press r to load", theme::muted())))]
    };

    f.render_widget(List::new(items), inner);
}
