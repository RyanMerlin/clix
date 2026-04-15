use anyhow::Result;
use clix_serve::build_serve_state;

pub async fn run(socket: Option<String>, http: Option<String>) -> Result<()> {
    // Hint if broker is not running
    let broker_socket_path = std::env::var("CLIX_BROKER_SOCKET")
        .unwrap_or_else(|_| "/tmp/clix-broker.sock".to_string());
    if !std::path::Path::new(&broker_socket_path).exists() {
        eprintln!("warn: broker socket not found — run `clix broker start` or `clix broker install-unit` for persistent credential minting");
    }

    let serve = build_serve_state()?;
    match (socket, http) {
        (Some(_path), _) => {
            #[cfg(unix)]
            { clix_serve::transport::socket::serve_socket(serve, &_path).await?; }
            #[cfg(not(unix))]
            anyhow::bail!("Unix socket transport not supported on Windows");
        }
        (_, Some(addr)) => {
            clix_serve::transport::http::serve_http(serve, &addr).await?;
        }
        _ => {
            clix_serve::transport::stdio::serve_stdio(serve).await?;
        }
    }
    Ok(())
}
