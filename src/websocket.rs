use futures_util::{SinkExt, StreamExt};
use native_tls::TlsConnector;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async_tls_with_config,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
    Connector,
};

use super::auth::Credentials;

// ─── Public types ────────────────────────────────────────────

/// A single LCU WebSocket event.
///
/// The LCU pushes events for every API state change. Each event carries:
/// - `uri` — the REST endpoint that changed (e.g. `/lol-gameflow/v1/session`)
/// - `event_type` — whether the resource was created, updated, or deleted
/// - `data` — the new state as arbitrary JSON
#[derive(Debug, Clone)]
pub struct LcuEvent {
    pub uri: String,
    pub event_type: EventType,
    pub data: Value,
}

/// Event type from the LCU WAMP payload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum EventType {
    Create,
    Update,
    Delete,
    #[serde(other)]
    Unknown,
}

// ─── Internal WAMP deserialization ───────────────────────────

/// WAMP Event frame payload: `{ uri, data, eventType }`
#[derive(Debug, Deserialize)]
struct WampPayload {
    uri: String,
    data: Value,
    #[serde(rename = "eventType")]
    event_type: EventType,
}

// ─── Core ────────────────────────────────────────────────────

/// Connect to the LCU WebSocket and subscribe to **all** JSON API events.
///
/// Returns an `mpsc::Receiver<LcuEvent>`. Every state change in the LCU is
/// delivered as an [`LcuEvent`] — the caller filters by `uri` as needed.
///
/// # Design — channel instead of callbacks
///
/// The original JS library dispatches events through a `Map<uri, callback[]>`.
/// This Rust port uses a `tokio::sync::mpsc` channel instead:
///
/// ```text
/// tokio task (owns WebSocket)
///     └─ tx.send(event) ──channel──► caller: rx.recv().await
/// ```
///
/// When the LCU disconnects, the background task drops `tx`, causing
/// `rx.recv()` to return `None` — a natural "connection closed" signal
/// with no extra bookkeeping.
///
/// # Arguments
///
/// - `credentials` — obtained via [`crate::authenticate`]
/// - `buffer` — mpsc channel capacity (64–256 recommended)
pub async fn connect(
    credentials: &Credentials,
    buffer: usize,
) -> Result<mpsc::Receiver<LcuEvent>, Box<dyn std::error::Error + Send + Sync>> {
    // TLS: skip certificate verification (LCU uses a Riot self-signed cert)
    let tls = TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()?;

    // WebSocket upgrade request with Basic Auth
    let mut request = credentials.lcu_ws_url().into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&credentials.basic_auth())?,
    );

    let (mut ws_stream, _response) = connect_async_tls_with_config(
        request,
        None,
        false,
        Some(Connector::NativeTls(tls)),
    )
    .await?;

    // WAMP Subscribe: [5, "OnJsonApiEvent"]
    // Subscribes to ALL JSON API events — the caller filters by URI.
    ws_stream
        .send(Message::Text(
            serde_json::json!([5, "OnJsonApiEvent"]).to_string().into(),
        ))
        .await?;

    let (tx, rx) = mpsc::channel::<LcuEvent>(buffer);

    tokio::spawn(async move {
        while let Some(msg) = ws_stream.next().await {
            let text = match msg {
                Ok(Message::Text(t)) => t,
                Ok(Message::Close(_)) | Err(_) => break,
                _ => continue,
            };

            if let Some(event) = parse_wamp_event(&text) {
                if tx.send(event).await.is_err() {
                    break; // receiver dropped
                }
            }
        }
        // tx drops here → rx.recv() returns None
    });

    Ok(rx)
}

// ─── Internal ────────────────────────────────────────────────

/// Parse a raw WAMP text frame into an [`LcuEvent`].
///
/// LCU event format: `[8, "OnJsonApiEvent", { uri, data, eventType }]`
/// (opcode 8 = WAMP EVENT)
fn parse_wamp_event(text: &str) -> Option<LcuEvent> {
    let arr: Vec<Value> = serde_json::from_str(text).ok()?;

    if arr.len() < 3 || arr[0].as_u64() != Some(8) {
        return None;
    }

    let payload: WampPayload = serde_json::from_value(arr.into_iter().nth(2)?).ok()?;

    Some(LcuEvent {
        uri: payload.uri,
        event_type: payload.event_type,
        data: payload.data,
    })
}

// ─── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_event() {
        let raw = r#"[8,"OnJsonApiEvent",{"uri":"/lol-gameflow/v1/session","eventType":"Update","data":{"phase":"Lobby"}}]"#;
        let event = parse_wamp_event(raw).unwrap();
        assert_eq!(event.uri, "/lol-gameflow/v1/session");
        assert_eq!(event.event_type, EventType::Update);
        assert_eq!(event.data["phase"], "Lobby");
    }

    #[test]
    fn test_parse_delete_event() {
        let raw = r#"[8,"OnJsonApiEvent",{"uri":"/lol-lobby/v2/lobby","eventType":"Delete","data":null}]"#;
        let event = parse_wamp_event(raw).unwrap();
        assert_eq!(event.event_type, EventType::Delete);
    }

    #[test]
    fn test_ignores_non_event_frames() {
        // opcode 5 = Subscribe ACK, not an event
        assert!(parse_wamp_event(r#"[5,"OnJsonApiEvent"]"#).is_none());
        assert!(parse_wamp_event("not json").is_none());
        assert!(parse_wamp_event("[]").is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn test_connect_and_receive() {
        use crate::auth::try_find_lcu;

        let creds = try_find_lcu().expect("LCU not running");
        let mut rx = connect(&creds, 64).await.expect("WS connect failed");

        println!("Connected — waiting for LCU events...");
        while let Some(event) = rx.recv().await {
            let tag = match event.event_type {
                EventType::Create => "CREATE",
                EventType::Update => "UPDATE",
                EventType::Delete => "DELETE",
                EventType::Unknown => "?",
            };
            println!("[{tag}] {}", event.uri);
        }
        println!("Connection closed.");
    }
}
