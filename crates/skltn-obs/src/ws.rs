use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use http::StatusCode;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::drilldown::DrilldownTracker;
use crate::proxy::AppState;
use crate::savings::SavingsTracker;
use crate::tracker::CostTracker;

#[derive(Serialize)]
struct TypedMessage<'a, T: Serialize> {
    r#type: &'a str,
    data: &'a T,
}

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
    Ok(ws.on_upgrade(|socket| handle_ws(socket, state.tracker, state.savings_tracker, state.drilldown_tracker)))
}

async fn handle_ws(mut socket: WebSocket, tracker: CostTracker, savings_tracker: SavingsTracker, drilldown_tracker: DrilldownTracker) {
    // 1. Replay existing records + subscribe under the same lock (gap-free invariant)
    let (usage_records, mut usage_rx) = tracker.snapshot_and_subscribe().await;
    let (savings_records, mut savings_rx) = savings_tracker.snapshot_and_subscribe().await;
    let (drilldown_records, mut drilldown_rx) = drilldown_tracker.snapshot_and_subscribe().await;

    // Replay usage records
    for record in &usage_records {
        if send_typed(&mut socket, "usage", record).await.is_err() {
            return;
        }
    }

    // Replay savings records
    for record in &savings_records {
        if send_typed(&mut socket, "savings", record).await.is_err() {
            return;
        }
    }

    // Replay drilldown records
    for record in &drilldown_records {
        if send_typed(&mut socket, "drilldown", record).await.is_err() {
            return;
        }
    }

    // 2. Stream live records from all channels
    loop {
        tokio::select! {
            result = usage_rx.recv() => {
                match result {
                    Ok(record) => {
                        if send_typed(&mut socket, "usage", &record).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client lagged by {n} usage messages, dropping connection");
                        return;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            result = savings_rx.recv() => {
                match result {
                    Ok(record) => {
                        if send_typed(&mut socket, "savings", &record).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client lagged by {n} savings messages, dropping connection");
                        return;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            result = drilldown_rx.recv() => {
                match result {
                    Ok(record) => {
                        if send_typed(&mut socket, "drilldown", &record).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client lagged by {n} drilldown messages, dropping connection");
                        return;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        }
    }
}

async fn send_typed<T: Serialize>(
    socket: &mut WebSocket,
    msg_type: &str,
    data: &T,
) -> Result<(), ()> {
    let envelope = TypedMessage {
        r#type: msg_type,
        data,
    };
    let json = match serde_json::to_string(&envelope) {
        Ok(j) => j,
        Err(_) => return Ok(()), // skip malformed, don't disconnect
    };
    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(|_| ())
}
