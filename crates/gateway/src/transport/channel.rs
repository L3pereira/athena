//! Tokio channel-based transport for single-process mode
//!
//! Uses broadcast channels for pub/sub semantics within a single process.
//! No serialization overhead - messages are passed directly.

use crate::error::TransportError;
use crate::transport::{Publisher, Requester, Subscriber};
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use std::marker::PhantomData;
use tokio::sync::{broadcast, mpsc, oneshot};

/// Channel-based publisher using broadcast
pub struct ChannelPublisher<M> {
    tx: broadcast::Sender<M>,
}

impl<M: Clone> ChannelPublisher<M> {
    /// Create a new publisher with the given broadcast sender
    pub fn new(tx: broadcast::Sender<M>) -> Self {
        Self { tx }
    }

    /// Create a publisher/subscriber pair with given capacity
    pub fn pair(capacity: usize) -> (Self, ChannelSubscriber<M>) {
        let (tx, rx) = broadcast::channel(capacity);
        (Self { tx: tx.clone() }, ChannelSubscriber { rx, _tx: tx })
    }

    /// Get another subscriber for this publisher
    pub fn subscribe(&self) -> ChannelSubscriber<M> {
        ChannelSubscriber {
            rx: self.tx.subscribe(),
            _tx: self.tx.clone(),
        }
    }
}

#[async_trait]
impl<M> Publisher<M> for ChannelPublisher<M>
where
    M: Serialize + Clone + Send + Sync + 'static,
{
    async fn publish(&self, msg: &M) -> Result<(), TransportError> {
        self.tx
            .send(msg.clone())
            .map_err(|_| TransportError::ChannelClosed)?;
        Ok(())
    }
}

/// Channel-based subscriber using broadcast receiver
pub struct ChannelSubscriber<M> {
    rx: broadcast::Receiver<M>,
    // Keep sender alive to prevent channel from closing
    _tx: broadcast::Sender<M>,
}

impl<M: Clone> ChannelSubscriber<M> {
    /// Create a new subscriber from a broadcast receiver
    pub fn new(rx: broadcast::Receiver<M>, tx: broadcast::Sender<M>) -> Self {
        Self { rx, _tx: tx }
    }
}

#[async_trait]
impl<M> Subscriber<M> for ChannelSubscriber<M>
where
    M: DeserializeOwned + Clone + Send + 'static,
{
    async fn next(&mut self) -> Result<M, TransportError> {
        loop {
            match self.rx.recv().await {
                Ok(msg) => return Ok(msg),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Skip lagged messages and continue
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(TransportError::ChannelClosed);
                }
            }
        }
    }

    fn try_next(&mut self) -> Result<Option<M>, TransportError> {
        match self.rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                // Return None on lag, caller can retry
                Ok(None)
            }
            Err(broadcast::error::TryRecvError::Closed) => Err(TransportError::ChannelClosed),
        }
    }
}

/// Request message wrapper for channel-based request/reply
struct ChannelRequest<Req, Res> {
    request: Req,
    reply_tx: oneshot::Sender<Res>,
}

/// Channel-based requester for request/reply pattern
pub struct ChannelRequester<Req, Res> {
    tx: mpsc::Sender<ChannelRequest<Req, Res>>,
}

impl<Req, Res> ChannelRequester<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    /// Create a requester/responder pair
    pub fn pair(capacity: usize) -> (Self, ChannelResponder<Req, Res>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, ChannelResponder { rx })
    }
}

#[async_trait]
impl<Req, Res> Requester<Req, Res> for ChannelRequester<Req, Res>
where
    Req: Serialize + Clone + Send + Sync + 'static,
    Res: DeserializeOwned + Send + 'static,
{
    async fn request(&self, req: &Req) -> Result<Res, TransportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let request = ChannelRequest {
            request: req.clone(),
            reply_tx,
        };

        self.tx
            .send(request)
            .await
            .map_err(|_| TransportError::ChannelClosed)?;

        reply_rx.await.map_err(|_| TransportError::ChannelClosed)
    }
}

/// Channel-based responder (server side of request/reply)
pub struct ChannelResponder<Req, Res> {
    rx: mpsc::Receiver<ChannelRequest<Req, Res>>,
}

impl<Req, Res> ChannelResponder<Req, Res> {
    /// Receive the next request
    pub async fn next(&mut self) -> Option<(Req, oneshot::Sender<Res>)> {
        self.rx.recv().await.map(|req| (req.request, req.reply_tx))
    }
}

/// Factory for creating channel-based transport components
pub struct ChannelTransportFactory<M> {
    _phantom: PhantomData<M>,
}

impl<M: Clone + Send + 'static> ChannelTransportFactory<M> {
    /// Create a pub/sub pair with default capacity (1000)
    pub fn pubsub() -> (ChannelPublisher<M>, ChannelSubscriber<M>) {
        ChannelPublisher::pair(1000)
    }

    /// Create a pub/sub pair with custom capacity
    pub fn pubsub_with_capacity(capacity: usize) -> (ChannelPublisher<M>, ChannelSubscriber<M>) {
        ChannelPublisher::pair(capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pubsub() {
        let (publisher, mut subscriber) = ChannelPublisher::<String>::pair(10);

        publisher.publish(&"hello".to_string()).await.unwrap();

        let msg = subscriber.next().await.unwrap();
        assert_eq!(msg, "hello");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let (publisher, mut sub1) = ChannelPublisher::<i32>::pair(10);
        let mut sub2 = publisher.subscribe();

        publisher.publish(&42).await.unwrap();

        assert_eq!(sub1.next().await.unwrap(), 42);
        assert_eq!(sub2.next().await.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_request_reply() {
        let (requester, mut responder) = ChannelRequester::<String, String>::pair(10);

        // Spawn responder task
        let handle = tokio::spawn(async move {
            if let Some((req, reply_tx)) = responder.next().await {
                let response = format!("Echo: {}", req);
                let _ = reply_tx.send(response);
            }
        });

        let response = requester.request(&"test".to_string()).await.unwrap();
        assert_eq!(response, "Echo: test");

        handle.await.unwrap();
    }
}
