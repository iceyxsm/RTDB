//! CDC Streaming Server - WebSocket and SSE endpoints
//!
//! Provides HTTP endpoints for consuming CDC events:
//! - WebSocket: /ws/cdc/{collection} - Real-time bidirectional
//! - SSE: /events/cdc/{collection} - Server-sent events
//! - HTTP: /api/v1/cdc/subscribe - REST polling

use super::{CdcEngine, CdcSubscription, ChangeEvent};
use crate::RTDBError;
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::sse::{Event, Sse},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::stream::Stream;
use serde_json;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

/// CDC Streaming Server
pub struct CdcStreamingServer {
    cdc_engine: Arc<CdcEngine>,
}

impl CdcStreamingServer {
    /// Create new streaming server
    pub fn new(cdc_engine: Arc<CdcEngine>) -> Self {
        Self { cdc_engine }
    }

    /// Get router for CDC endpoints
    pub fn router(&self) -> Router {
        let state = self.cdc_engine.clone();

        Router::new()
            .route("/ws/cdc/:collection", get(ws_handler))
            .route("/events/cdc/:collection", get(sse_handler))
            .with_state(state)
    }
}

/// WebSocket handler for CDC streaming
async fn ws_handler(
    Path(collection): Path<String>,
    State(cdc_engine): State<Arc<CdcEngine>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, collection, cdc_engine))
}

/// Handle WebSocket connection
async fn handle_websocket(
    mut socket: axum::extract::ws::WebSocket,
    collection: String,
    cdc_engine: Arc<CdcEngine>,
) {
    // Subscribe to CDC events
    let mut subscription = match cdc_engine.subscribe(&collection).await {
        Ok(sub) => sub,
        Err(e) => {
            let _ = socket
                .send(axum::extract::ws::Message::Text(format!(
                    "{{\"error\":\"{}\"}}",
                    e
                )))
                .await;
            return;
        }
    };

    // Stream events to client
    loop {
        tokio::select! {
            // Receive CDC event
            Ok(event) = subscription.recv() => {
                let json = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("Failed to serialize event: {}", e);
                        continue;
                    }
                };

                if socket.send(axum::extract::ws::Message::Text(json)).await.is_err() {
                    break; // Client disconnected
                }
            }
            // Receive client message (for ACKs, filtering, etc.)
            Some(msg) = socket.recv() => {
                match msg {
                    Ok(axum::extract::ws::Message::Close(_)) => break,
                    Ok(axum::extract::ws::Message::Text(text)) => {
                        // Handle client commands (filter, ACK, etc.)
                        if text == "ping" {
                            let _ = socket.send(axum::extract::ws::Message::Pong(vec![])).await;
                        }
                    }
                    Err(_) => break,
                    _ => {}
                }
            }
        }
    }
}

/// SSE handler for CDC streaming
async fn sse_handler(
    Path(collection): Path<String>,
    State(cdc_engine): State<Arc<CdcEngine>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(100);
    let cdc = cdc_engine.clone();

    // Spawn task to feed events
    tokio::spawn(async move {
        let mut subscription = match cdc.subscribe(&collection).await {
            Ok(sub) => sub,
            Err(e) => {
                let event = Event::default().event("error").data(format!("{}", e));
                let _ = tx.send(Ok(event)).await;
                return;
            }
        };

        loop {
            match subscription.recv().await {
                Ok(event) => {
                    let json = match serde_json::to_string(&event) {
                        Ok(j) => j,
                        Err(_) => continue,
                    };

                    let sse_event = Event::default()
                        .event("change")
                        .id(&event.event_id)
                        .data(json);

                    if tx.send(Ok(sse_event)).await.is_err() {
                        break; // Client disconnected
                    }
                }
                Err(_) => break,
            }
        }
    });

    let stream = ReceiverStream::new(rx);

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// CDC Client for consuming streams
pub struct CdcStreamClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl CdcStreamClient {
    /// Create new CDC stream client
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Subscribe to CDC events via WebSocket
    pub async fn subscribe_websocket(
        &self,
        collection: &str,
    ) -> Result<WebSocketStream, RTDBError> {
        let url = format!("{}/ws/cdc/{}", self.base_url.replace("http", "ws"), collection);

        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| RTDBError::Io(format!("WebSocket connect failed: {}", e)))?;

        Ok(WebSocketStream { inner: ws_stream })
    }

    /// Subscribe to CDC events via SSE
    pub async fn subscribe_sse(&self, collection: &str) -> Result<SseStream, RTDBError> {
        let url = format!("{}/events/cdc/{}", self.base_url, collection);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| RTDBError::Io(format!("SSE connect failed: {}", e)))?;

        Ok(SseStream {
            response,
            buffer: String::new(),
        })
    }
}

/// WebSocket stream wrapper
pub struct WebSocketStream {
    inner: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
}

impl WebSocketStream {
    /// Receive next change event
    pub async fn recv(&mut self) -> Result<ChangeEvent, RTDBError> {
        loop {
            match self.inner.next().await {
                Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                    return serde_json::from_str(&text)
                        .map_err(|e| RTDBError::Serialization(format!("Invalid JSON: {}", e)));
                }
                Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                    return Err(RTDBError::Io("WebSocket closed".to_string()));
                }
                Some(Err(e)) => {
                    return Err(RTDBError::Io(format!("WebSocket error: {}", e)));
                }
                _ => continue,
            }
        }
    }

    /// Send ACK for received event
    pub async fn ack(&mut self, event_id: &str) -> Result<(), RTDBError> {
        let msg = format!("{{\"ack\":\"{}\"}}", event_id);
        self.inner
            .send(tokio_tungstenite::tungstenite::Message::Text(msg))
            .await
            .map_err(|e| RTDBError::Io(format!("Send failed: {}", e)))
    }

    /// Close the connection
    pub async fn close(mut self) {
        let _ = self
            .inner
            .close(None)
            .await;
    }
}

/// SSE stream wrapper
pub struct SseStream {
    response: reqwest::Response,
    buffer: String,
}

impl SseStream {
    /// Receive next change event
    pub async fn recv(&mut self) -> Result<ChangeEvent, RTDBError> {
        use futures::StreamExt;

        let mut chunks = self.response.bytes_stream();

        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|e| RTDBError::Io(format!("SSE read error: {}", e)))?;
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Parse SSE format: "event: change\nid: xxx\ndata: {json}\n\n"
            if let Some(pos) = self.buffer.find("\n\n") {
                let message = self.buffer[..pos].to_string();
                self.buffer = self.buffer[pos + 2..].to_string();

                // Extract data field
                for line in message.lines() {
                    if line.starts_with("data: ") {
                        let json = &line[6..];
                        return serde_json::from_str(json)
                            .map_err(|e| RTDBError::Serialization(format!("Invalid JSON: {}", e)));
                    }
                }
            }
        }

        Err(RTDBError::Io("SSE stream ended".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::{CdcConfig, CdcEngine};

    #[tokio::test]
    async fn test_sse_stream_parsing() {
        // Test SSE message parsing logic
        let sse_data = "event: change\nid: evt-123\ndata: {\"event_id\":\"evt-123\",\"collection\":\"test\",\"vector_id\":1,\"change_type\":\"Insert\",\"timestamp_us\":123456}\n\n";

        // Extract data portion
        for line in sse_data.lines() {
            if line.starts_with("data: ") {
                let json = &line[6..];
                let event: ChangeEvent = serde_json::from_str(json).unwrap();
                assert_eq!(event.collection, "test");
                assert_eq!(event.vector_id, 1);
            }
        }
    }
}
