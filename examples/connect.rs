//! Full demo: process discovery → WebSocket event subscription
//!
//! Run with: `cargo run --example connect`
//! (requires a running League Client)

use league_connect_rust::{authenticate, connect, try_find_lcu, EventType};

#[tokio::main]
async fn main() {
    println!("league-connect-rust — WebSocket event monitor\n");

    // Wait for the League Client
    let creds = match try_find_lcu() {
        Some(c) => {
            println!("[auth] LCU found  port={} pid={}", c.port, c.pid);
            c
        }
        None => {
            println!("[auth] LCU not running — polling every 3 s...");
            let c = authenticate(3000).await;
            println!("[auth] LCU found  port={} pid={}", c.port, c.pid);
            c
        }
    };

    // Connect to the LCU WebSocket (receives ALL events)
    println!("[ws]   Connecting...");
    let mut rx = connect(&creds, 128).await.expect("WebSocket connect failed");
    println!("[ws]   Subscribed to OnJsonApiEvent — listening...\n");

    while let Some(event) = rx.recv().await {
        let tag = match event.event_type {
            EventType::Create => "CREATE",
            EventType::Update => "UPDATE",
            EventType::Delete => "DELETE",
            EventType::Unknown => "?",
        };
        // Print every event URI — you'll see lobby, matchmaking, gameflow, etc.
        println!("[{tag:>6}]  {}", event.uri);
    }

    println!("\n[ws]   Connection closed.");
}
