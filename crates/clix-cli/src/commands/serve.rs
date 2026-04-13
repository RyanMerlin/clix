use anyhow::Result;
use clix_serve::build_serve_state;

pub async fn run(socket: Option<String>, http: Option<String>) -> Result<()> {
    let serve = build_serve_state()?;
    match (socket, http) {
        (Some(path), _) => {
            #[cfg(unix)]
            { clix_serve::transport::socket::serve_socket(serve, &path).await?; }
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
