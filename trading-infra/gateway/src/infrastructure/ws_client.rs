use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::parsers::StreamDataParser;
use crate::domain::{WsEvent, WsRequest, WsResponse};

#[derive(Error, Debug)]
pub enum WsError {
    #[error("Connection error: {0}")]
    Connection(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Not connected")]
    NotConnected,
}

/// WebSocket client for streaming market data
/// Infrastructure component - handles WebSocket communication
pub struct WsClient {
    url: String,
}

impl WsClient {
    pub fn new(url: String) -> Self {
        WsClient { url }
    }

    /// Connect and return channels for sending requests and receiving events
    pub async fn connect(&self) -> Result<(WsRequestSender, mpsc::Receiver<WsEvent>), WsError> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Channel for sending requests to the WebSocket
        let (req_tx, mut req_rx) = mpsc::channel::<WsRequest>(32);

        // Channel for receiving events from the WebSocket
        let (event_tx, event_rx) = mpsc::channel::<WsEvent>(1024);

        // Spawn task to handle outgoing messages
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            while let Some(req) = req_rx.recv().await {
                let json = match serde_json::to_string(&req) {
                    Ok(j) => j,
                    Err(e) => {
                        let _ = event_tx_clone.send(WsEvent::Error(e.to_string())).await;
                        continue;
                    }
                };

                if let Err(e) = write.send(Message::Text(json.into())).await {
                    let _ = event_tx_clone.send(WsEvent::Error(e.to_string())).await;
                    break;
                }
            }
        });

        // Spawn task to handle incoming messages
        // Parser is created in infrastructure layer - keeps domain free of concrete dependencies
        let parser = StreamDataParser::new();

        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => match serde_json::from_str::<WsResponse>(&text) {
                        Ok(response) => {
                            let event = match &response {
                                WsResponse::Result { id, result } => WsEvent::Response {
                                    id: *id,
                                    result: result.clone(),
                                },
                                WsResponse::Stream { stream, data } => {
                                    if let Some(stream_data) = parser.parse(stream, data) {
                                        WsEvent::StreamData(stream_data)
                                    } else {
                                        WsEvent::RawMessage(text.to_string())
                                    }
                                }
                                WsResponse::Error { id, code, msg } => WsEvent::ApiError {
                                    id: *id,
                                    code: *code,
                                    msg: msg.clone(),
                                },
                            };

                            if event_tx.send(event).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            if event_tx
                                .send(WsEvent::RawMessage(text.to_string()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    },
                    Ok(Message::Close(_)) => {
                        let _ = event_tx.send(WsEvent::Disconnected).await;
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        tracing::trace!("Received ping: {:?}", data);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        let _ = event_tx.send(WsEvent::Error(e.to_string())).await;
                        break;
                    }
                }
            }
        });

        Ok((
            WsRequestSender {
                tx: req_tx,
                request_id: Arc::new(AtomicU64::new(1)),
            },
            event_rx,
        ))
    }
}

/// Handle for sending WebSocket requests
#[derive(Clone)]
pub struct WsRequestSender {
    tx: mpsc::Sender<WsRequest>,
    request_id: Arc<AtomicU64>,
}

impl WsRequestSender {
    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Subscribe to streams
    pub async fn subscribe(&self, streams: Vec<String>) -> Result<u64, WsError> {
        let id = self.next_id();
        self.tx
            .send(WsRequest::subscribe(id, streams))
            .await
            .map_err(|_| WsError::ChannelClosed)?;
        Ok(id)
    }

    /// Unsubscribe from streams
    pub async fn unsubscribe(&self, streams: Vec<String>) -> Result<u64, WsError> {
        let id = self.next_id();
        self.tx
            .send(WsRequest::unsubscribe(id, streams))
            .await
            .map_err(|_| WsError::ChannelClosed)?;
        Ok(id)
    }

    /// List current subscriptions
    pub async fn list_subscriptions(&self) -> Result<u64, WsError> {
        let id = self.next_id();
        self.tx
            .send(WsRequest::list_subscriptions(id))
            .await
            .map_err(|_| WsError::ChannelClosed)?;
        Ok(id)
    }
}
