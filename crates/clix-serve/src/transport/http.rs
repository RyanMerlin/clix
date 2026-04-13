use std::sync::Arc;
use axum::{extract::State, routing::post, Json, Router};
use crate::dispatch::{dispatch, ServeState};

pub async fn serve_http(serve: Arc<ServeState>, addr: &str) -> anyhow::Result<()> {
    let app = Router::new().route("/", post(handle_rpc)).with_state(serve);
    eprintln!("clix daemon listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_rpc(State(serve): State<Arc<ServeState>>, Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(dispatch(serve, req).await)
}
