//! 端到端连接示例：进程发现 → REST → WebSocket 事件订阅

use league_connect_rust::{
    auth::{authenticate, try_find_lcu},
    http::{build_lcu_client, lcu_get, parse_marketing_version},
    websocket::{connect, EventType},
};

#[tokio::main]
async fn main() {
    println!("=== league-connect-rust demo ===\n");

    // ── 1. 进程发现 ─────────────────────────────────────────────
    let creds = match try_find_lcu() {
        Some(c) => {
            println!("[auth] LCU running  PID={} Port={}", c.pid, c.port);
            c
        }
        None => {
            println!("[auth] LCU not running, polling every 3s...");
            let c = authenticate(3000).await;
            println!("[auth] Found!  PID={} Port={}", c.pid, c.port);
            c
        }
    };

    // ── 2. REST 请求 ─────────────────────────────────────────────
    let client = build_lcu_client();

    print!("[http] GET /lol-patch/v1/game-version  →  ");
    match lcu_get(&client, &creds, "/lol-patch/v1/game-version").await {
        Some(v) => {
            let raw = v.as_str().unwrap_or("?");
            let patch = parse_marketing_version(raw).unwrap_or_default();
            println!("raw={raw}  marketing={patch}");
        }
        None => println!("(no response — LCU may still be starting)"),
    }

    print!("[http] GET /lol-gameflow/v1/session  →  ");
    match lcu_get(&client, &creds, "/lol-gameflow/v1/session").await {
        Some(v) => println!("phase={}", v["phase"]),
        None => println!("(no active session)"),
    }

    // ── 3. WebSocket 事件订阅 ────────────────────────────────────
    println!("\n[ws]  Connecting...");
    let mut rx = connect(&creds, 64).await.expect("WebSocket connect failed");
    println!("[ws]  Connected. Enter champion select to see events.\n");

    while let Some(event) = rx.recv().await {
        let tag = match event.event_type {
            EventType::Create  => "CREATE",
            EventType::Update  => "UPDATE",
            EventType::Delete  => "DELETE",
            EventType::Unknown => "?",
        };
        // Print every event URI; filter as needed for your use case
        println!("[{tag}] {}", event.uri);

        if event.uri == "/lol-champ-select/v1/session" {
            if event.event_type == EventType::Delete {
                println!("      └─ session ended");
            } else {
                let phase = &event.data["timer"]["phase"];
                println!("      └─ phase={phase}");
            }
        }
    }

    println!("[ws]  Connection closed.");
}
