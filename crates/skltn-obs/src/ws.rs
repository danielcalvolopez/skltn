use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use http::StatusCode;
use tokio::sync::broadcast;

use crate::proxy::AppState;
use crate::tracker::CostTracker;

pub async fn ws_handler(
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validate Origin header to prevent cross-site WebSocket hijacking.
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        let allowed = origin.starts_with("http://localhost:")
            || origin.starts_with("http://127.0.0.1:")
            || origin.starts_with("http://[::1]:");
        if !allowed {
            tracing::warn!("Rejected WebSocket connection from origin: {origin}");
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(ws.on_upgrade(|socket| handle_ws(socket, state.tracker)))
}

async fn handle_ws(mut socket: WebSocket, tracker: CostTracker) {
    // 1. Replay existing records + subscribe under the same lock (gap-free invariant)
    let (records, mut rx) = tracker.snapshot_and_subscribe().await;

    for record in records {
        let msg = match serde_json::to_string(&record) {
            Ok(json) => json,
            Err(_) => continue,
        };
        if socket.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
    }

    // 2. Stream live records
    loop {
        match rx.recv().await {
            Ok(record) => {
                let msg = match serde_json::to_string(&record) {
                    Ok(json) => json,
                    Err(_) => continue,
                };
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("WebSocket client lagged by {n} messages, dropping connection");
                return;
            }
            Err(broadcast::error::RecvError::Closed) => {
                return;
            }
        }
    }
}
