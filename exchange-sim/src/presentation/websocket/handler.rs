use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;

use crate::application::ports::WebSocketRateLimiter;
use crate::domain::Clock;
use crate::infrastructure::TokenBucketRateLimiter;

use super::StreamManager;
use super::message::{WsRequest, WsResponse};

/// WebSocket connection state
pub struct WsState<C: Clock> {
    pub clock: Arc<C>,
    pub stream_manager: Arc<StreamManager>,
    pub rate_limiter: Arc<TokenBucketRateLimiter>,
}

/// Handle WebSocket upgrade
pub async fn ws_handler<C: Clock + 'static>(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WsState<C>>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle WebSocket connection
async fn handle_socket<C: Clock>(socket: WebSocket, state: Arc<WsState<C>>) {
    let (mut sender, mut receiver) = socket.split();

    // Track subscriptions
    let subscriptions: Arc<parking_lot::Mutex<HashSet<String>>> =
        Arc::new(parking_lot::Mutex::new(HashSet::new()));

    // Channel for outgoing messages
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(100);

    // Spawn task to forward messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Spawn tasks for each subscription
    let stream_manager = Arc::clone(&state.stream_manager);
    let subs = Arc::clone(&subscriptions);
    let tx_clone = tx.clone();

    // Handle incoming messages
    let rate_limiter = Arc::clone(&state.rate_limiter);
    let client_id = "ws-client"; // Simplified - in production, use connection ID

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            // Rate limit check
            let rate_result = rate_limiter.check_ws_message(client_id).await;
            if !rate_result.allowed {
                let error = WsResponse::error(None, -1015, "Too many messages");
                if let Ok(json) = serde_json::to_string(&error) {
                    let _ = tx.send(json).await;
                }
                continue;
            }

            // Parse request
            let request: Result<WsRequest, _> = serde_json::from_str(&text);
            match request {
                Ok(WsRequest::Subscribe { id, params }) => {
                    for stream in &params {
                        if let Some(mut event_rx) = stream_manager.subscribe(stream) {
                            subs.lock().insert(stream.clone());

                            // Spawn task to forward stream events
                            let tx = tx_clone.clone();
                            let stream_name = stream.clone();
                            let manager = Arc::clone(&stream_manager);

                            tokio::spawn(async move {
                                while let Ok(event) = event_rx.recv().await {
                                    if let Some(msg) =
                                        manager.event_to_message(&stream_name, &event)
                                    {
                                        let response = WsResponse::Stream {
                                            stream: msg.stream,
                                            data: msg.data,
                                        };
                                        if let Ok(json) = serde_json::to_string(&response) {
                                            if tx.send(json).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }

                    let response = WsResponse::ok(id);
                    if let Ok(json) = serde_json::to_string(&response) {
                        let _ = tx.send(json).await;
                    }
                }
                Ok(WsRequest::Unsubscribe { id, params }) => {
                    for stream in &params {
                        stream_manager.unsubscribe(stream);
                        subs.lock().remove(stream);
                    }

                    let response = WsResponse::ok(id);
                    if let Ok(json) = serde_json::to_string(&response) {
                        let _ = tx.send(json).await;
                    }
                }
                Ok(WsRequest::ListSubscriptions { id }) => {
                    let current_subs: Vec<String> = subs.lock().iter().cloned().collect();
                    let response = WsResponse::subscriptions(id, current_subs);
                    if let Ok(json) = serde_json::to_string(&response) {
                        let _ = tx.send(json).await;
                    }
                }
                Err(e) => {
                    let error = WsResponse::error(None, -1, format!("Invalid request: {}", e));
                    if let Ok(json) = serde_json::to_string(&error) {
                        let _ = tx.send(json).await;
                    }
                }
            }
        }
    }

    // Cleanup
    drop(tx);
    let _ = send_task.await;
}
