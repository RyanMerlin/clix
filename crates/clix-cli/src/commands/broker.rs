use clap::Subcommand;
use anyhow::{Result, Context};
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum BrokerCmd {
    /// Start the broker daemon
    Start {
        /// Run in foreground (for systemd / debugging)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the broker daemon
    Stop,
    /// Show broker status and adopted credentials
    Status,
    /// List adopted credentials
    ListCreds,
    /// Install systemd user unit for persistent broker (Linux only)
    InstallUnit,
}

pub fn run_broker(cmd: BrokerCmd) -> Result<()> {
    match cmd {
        BrokerCmd::Start { foreground } => start_broker(foreground),
        BrokerCmd::Stop => stop_broker(),
        BrokerCmd::Status => broker_status(),
        BrokerCmd::ListCreds => list_creds(),
        BrokerCmd::InstallUnit => install_unit(),
    }
}

fn broker_bin() -> Result<PathBuf> {
    // Look next to current executable first
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.parent().map(|d| d.join("clix-broker")).unwrap_or_default();
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    // Then PATH
    let path_var = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string());
    for dir in path_var.split(':') {
        let candidate = std::path::Path::new(dir).join("clix-broker");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("`clix-broker` binary not found next to clix or on PATH")
}

fn pid_file() -> PathBuf {
    creds_dir().parent().unwrap_or_else(|| std::path::Path::new("/tmp")).join("broker.pid")
}

fn creds_dir() -> PathBuf {
    std::env::var("CLIX_BROKER_CREDS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("/var/lib"))
                .join("clix")
                .join("broker")
        })
}

fn socket_path() -> String {
    std::env::var("CLIX_BROKER_SOCKET")
        .unwrap_or_else(|_| "/tmp/clix-broker.sock".to_string())
}

fn start_broker(foreground: bool) -> Result<()> {
    let bin = broker_bin()?;

    if foreground {
        // exec replacing this process
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&bin).exec();
        anyhow::bail!("exec {}: {err}", bin.display());
    }

    let child = std::process::Command::new(&bin)
        .spawn()
        .with_context(|| format!("spawn {}", bin.display()))?;
    let pid = child.id();

    // Write PID file
    let pid_path = pid_file();
    if let Some(parent) = pid_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&pid_path, pid.to_string())
        .with_context(|| format!("write PID file {}", pid_path.display()))?;

    println!("broker started (pid {pid})");
    Ok(())
}

fn stop_broker() -> Result<()> {
    let pid_path = pid_file();
    if !pid_path.exists() {
        anyhow::bail!("PID file not found at {} — is the broker running?", pid_path.display());
    }

    let pid_str = std::fs::read_to_string(&pid_path)
        .with_context(|| format!("read PID file {}", pid_path.display()))?;
    let pid: i32 = pid_str.trim().parse()
        .with_context(|| format!("parse PID from {pid_str:?}"))?;

    #[cfg(target_os = "linux")]
    {
        let ret = unsafe { libc::kill(pid, libc::SIGTERM) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("warn: kill pid {pid}: {err}");
        } else {
            println!("sent SIGTERM to broker (pid {pid})");
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("warn: SIGTERM not supported on this platform; remove PID file manually");
        let _ = pid;
    }

    let _ = std::fs::remove_file(&pid_path);
    let _ = std::fs::remove_file(socket_path());
    println!("broker stopped");
    Ok(())
}

fn broker_status() -> Result<()> {
    let pid_path = pid_file();
    let socket = socket_path();

    // Check PID
    let pid_info = if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path).unwrap_or_default();
        let pid: i32 = pid_str.trim().parse().unwrap_or(0);
        let alive = process_alive(pid);
        if alive {
            format!("running (pid {pid})")
        } else {
            format!("stale PID file (pid {pid} not running)")
        }
    } else {
        "not running (no PID file)".to_string()
    };

    // Try ping
    let ping_result = ping_broker(&socket);

    println!("broker:  {pid_info}");
    println!("socket:  {socket}");
    println!("ping:    {}", ping_result.unwrap_or_else(|e| format!("unreachable: {e}")));
    println!("creds:   {}", creds_dir().display());
    println!();

    // List creds inline
    print_creds_list();

    Ok(())
}

fn list_creds() -> Result<()> {
    print_creds_list();
    Ok(())
}

fn print_creds_list() {
    let creds = creds_dir();
    if !creds.exists() {
        println!("no credentials adopted (creds dir not found)");
        return;
    }

    let mut found = false;

    // gcloud ADC
    let adc = creds.join("gcloud").join("adc.json");
    if adc.exists() {
        found = true;
        let age = file_age(&adc);
        println!("  gcloud:adc        {}", age);
    }

    // gcloud SA registry
    let sa_registry = creds.join("gcloud").join("sa_registry.json");
    if sa_registry.exists() {
        if let Ok(text) = std::fs::read_to_string(&sa_registry) {
            if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                for entry in &entries {
                    found = true;
                    let email = entry["email"].as_str().unwrap_or("?");
                    let path = entry["path"].as_str().unwrap_or("?");
                    let adopted_at = entry["adopted_at"].as_str().unwrap_or("?");
                    println!("  gcloud:sa ({email})  path={path}  adopted={adopted_at}");
                }
            }
        }
    }

    // kubectl
    let kubeconfig = creds.join("kubectl").join("kubeconfig");
    if kubeconfig.exists() {
        found = true;
        let age = file_age(&kubeconfig);
        println!("  kubectl:kubeconfig  {}", age);
    }

    // generic
    if let Ok(entries) = std::fs::read_dir(&creds) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == "gcloud" || name_str == "kubectl" { continue; }
            let secret_env = entry.path().join("secret.env");
            if secret_env.exists() {
                found = true;
                let age = file_age(&secret_env);
                println!("  generic:{name_str}  {age}");
            }
        }
    }

    if !found {
        println!("  (no credentials adopted)");
    }
}

fn file_age(path: &std::path::Path) -> String {
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(duration) = std::time::SystemTime::now().duration_since(modified) {
                let secs = duration.as_secs();
                if secs < 60 { return format!("{}s ago", secs); }
                if secs < 3600 { return format!("{}m ago", secs / 60); }
                if secs < 86400 { return format!("{}h ago", secs / 3600); }
                return format!("{}d ago", secs / 86400);
            }
        }
    }
    "?".to_string()
}

fn process_alive(pid: i32) -> bool {
    if pid <= 0 { return false; }
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

fn ping_broker(socket: &str) -> Result<String> {
    use std::os::unix::net::UnixStream;
    use std::io::{BufRead, BufReader, Write};
    use std::time::Instant;

    let start = Instant::now();
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("connect to {socket}"))?;
    stream.write_all(b"{\"type\":\"ping\"}\n")
        .context("write ping")?;
    let reader = BufReader::new(&stream);
    match reader.lines().next() {
        Some(Ok(line)) if line.contains("pong") => {
            Ok(format!("{}ms", start.elapsed().as_millis()))
        }
        Some(Ok(line)) => anyhow::bail!("unexpected response: {line}"),
        _ => anyhow::bail!("no response"),
    }
}

fn install_unit() -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("systemd unit installation is only supported on Linux");

    #[cfg(target_os = "linux")]
    {
        let broker_bin = broker_bin()?;
        let home = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"));

        let unit_dir = home.join(".config").join("systemd").join("user");
        std::fs::create_dir_all(&unit_dir)
            .with_context(|| format!("create {}", unit_dir.display()))?;

        let unit_path = unit_dir.join("clix-broker.service");
        let unit_content = format!(
r#"[Unit]
Description=clix credential broker
After=default.target

[Service]
Type=simple
ExecStart={broker_bin}
Restart=on-failure
RestartSec=5
Environment=CLIX_HOME={home}/.clix
Environment=CLIX_BROKER_CREDS_DIR={home}/.local/share/clix/broker

[Install]
WantedBy=default.target
"#,
            broker_bin = broker_bin.display(),
            home = home.display(),
        );

        std::fs::write(&unit_path, &unit_content)
            .with_context(|| format!("write {}", unit_path.display()))?;
        println!("wrote {}", unit_path.display());

        // Reload and enable
        let reload = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        match reload {
            Ok(s) if s.success() => {
                let enable = std::process::Command::new("systemctl")
                    .args(["--user", "enable", "clix-broker"])
                    .status();
                match enable {
                    Ok(s) if s.success() => println!("clix-broker.service enabled"),
                    Ok(s) => eprintln!("systemctl enable exited {s}"),
                    Err(e) => eprintln!("systemctl enable: {e}"),
                }
            }
            Ok(s) => eprintln!("systemctl daemon-reload exited {s}"),
            Err(_) => {
                println!("systemctl not available. To activate manually:");
                println!("  systemctl --user daemon-reload");
                println!("  systemctl --user enable --now clix-broker");
            }
        }

        Ok(())
    }
}
