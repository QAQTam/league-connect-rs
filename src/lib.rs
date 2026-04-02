//! # league-connect-rust
//!
//! Rust client for the **League of Legends Client Update (LCU) API**.
//!
//! A Rust port of the [league-connect](https://github.com/matsjla/league-connect)
//! Node.js library, providing:
//!
//! - **Process discovery** — find the running League Client and extract its
//!   API credentials (port + auth token) without spawning shell processes.
//! - **REST client** — `GET` / `POST` / `PUT` / `DELETE` any LCU endpoint,
//!   with automatic self-signed certificate handling.
//! - **WebSocket (WAMP)** — subscribe to all LCU events through a
//!   `tokio::sync::mpsc` channel.
//!
//! ## Quick start
//!
//! ```no_run
//! use league_connect_rust::{authenticate, build_lcu_client, lcu_get, connect, EventType};
//!
//! #[tokio::main]
//! async fn main() {
//!     // 1. Wait for the League Client
//!     let creds = authenticate(3000).await;
//!
//!     // 2. REST — any LCU endpoint
//!     let client = build_lcu_client();
//!     let summoner = lcu_get(&client, &creds, "/lol-summoner/v1/current-summoner").await;
//!     println!("{:?}", summoner);
//!
//!     // 3. WebSocket — receive ALL LCU events, filter by URI
//!     let mut rx = connect(&creds, 128).await.unwrap();
//!     while let Some(ev) = rx.recv().await {
//!         println!("[{:?}] {}", ev.event_type, ev.uri);
//!     }
//! }
//! ```

pub mod auth;
pub mod http;
pub mod websocket;

pub use auth::{authenticate, try_find_lcu, Credentials};
pub use http::{build_lcu_client, lcu_delete, lcu_get, lcu_post, lcu_put, parse_marketing_version};
pub use websocket::{connect, EventType, LcuEvent};
