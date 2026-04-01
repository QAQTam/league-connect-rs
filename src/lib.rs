//! # league-connect-rust
//!
//! Rust port of the [league-connect](https://github.com/matsjla/league-connect)
//! Node.js library. Provides process discovery, HTTP REST, and WebSocket
//! (WAMP) access to the League of Legends Client Update (LCU) API.
//!
//! ## Quick start
//!
//! ```no_run
//! use league_connect_rust::auth::authenticate;
//! use league_connect_rust::http::{build_lcu_client, lcu_get};
//! use league_connect_rust::websocket::{connect, EventType};
//!
//! #[tokio::main]
//! async fn main() {
//!     // 1. Wait for the League Client to launch
//!     let creds = authenticate(3000).await;
//!     println!("LCU port={} pid={}", creds.port, creds.pid);
//!
//!     // 2. Make a REST request
//!     let client = build_lcu_client();
//!     let version = lcu_get(&client, &creds, "/lol-patch/v1/game-version").await;
//!     println!("version: {:?}", version);
//!
//!     // 3. Subscribe to WebSocket events
//!     let mut rx = connect(&creds, 64).await.expect("WS connect failed");
//!     while let Some(event) = rx.recv().await {
//!         if event.uri == "/lol-champ-select/v1/session" {
//!             println!("[{:?}] {}", event.event_type, event.data);
//!         }
//!     }
//! }
//! ```

pub mod auth;
pub mod http;
pub mod websocket;

// Re-export the most commonly used types at the crate root
pub use auth::{authenticate, try_find_lcu, Credentials};
pub use http::{build_lcu_client, lcu_get, parse_marketing_version};
pub use websocket::{connect, EventType, LcuEvent};
