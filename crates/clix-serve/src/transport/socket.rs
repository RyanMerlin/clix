use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::dispatch::ServeState;
use crate::transport::stdio::process_line;

#[cfg(unix)]
pub async fn serve_socket(serve: Arc<ServeState>, path: &str) -> anyhow::Result<()> {
    let _ = std::fs::remove_file(path);
    let listener = tokio::net::UnixListener::bind(path)?;
    eprintln!("clix daemon listening on unix:{path}");
    loop {
        let (stream, _) = listener.accept().await?;
        let serve = Arc::clone(&serve);
        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if let Some(resp) = process_line(Arc::clone(&serve), &line).await {
                            let _ = writer.write_all(resp.as_bytes()).await;
                            let _ = writer.write_all(b"\n").await;
                            let _ = writer.flush().await;
                        }
                    }
                }
            }
        });
    }
}
