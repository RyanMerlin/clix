use std::sync::Arc;
use axum::{extract::State, routing::{post, get}, Json, Router};
use crate::dispatch::{dispatch, ServeState};

pub async fn serve_http(serve: Arc<ServeState>, addr: &str) -> anyhow::Result<()> {
    if std::env::var("CLIX_HTTP_EXPERIMENTAL").is_err() {
        anyhow::bail!(
            "HTTP transport has no authentication or TLS and must not be exposed to untrusted networks.\n\
             Set CLIX_HTTP_EXPERIMENTAL=1 to acknowledge this and start anyway.\n\
             For local use, prefer the Unix socket transport (--socket) or the stdio transport (default)."
        );
    }
    eprintln!("[clix-serve] WARNING: HTTP transport is experimental — no auth, no TLS");
    eprintln!("[clix-serve] Do not expose this to untrusted networks (CLIX_HTTP_EXPERIMENTAL=1 acknowledged)");
    let app = Router::new()
        .route("/", post(handle_rpc))
        .route("/metrics", get(metrics_handler))
        .with_state(serve);
    eprintln!("clix daemon listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_rpc(State(serve): State<Arc<ServeState>>, Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(dispatch(serve, req).await)
}

async fn metrics_handler() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        crate::metrics::render(),
    )
}
