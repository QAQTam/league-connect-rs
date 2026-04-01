# league-connect-rust

Rust port of [league-connect](https://github.com/matsjla/league-connect) — a client library for the **League of Legends Client Update (LCU) API**.

Provides three capabilities that directly mirror the original Node.js library:

| Feature | JS original | Rust implementation |
|---|---|---|
| Process discovery | PowerShell `Get-CimInstance` | `sysinfo` crate (direct syscall) |
| LCU REST requests | `https.request` + `rejectUnauthorized: false` | `reqwest` + `native-tls` |
| LCU WebSocket (WAMP) | `ws` + `[5, "OnJsonApiEvent"]` | `tokio-tungstenite` + `mpsc` channel |

> **Windows only** — the LCU runs on Windows (and macOS/Linux for the standalone client), but this library currently targets Windows process discovery. The HTTP and WebSocket modules are cross-platform.

---

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
league-connect-rust = { path = "../league-connect-rust" }
tokio = { version = "1", features = ["full"] }
```

### 1 — Process discovery

```rust
use league_connect_rust::{try_find_lcu, authenticate};

// One-shot: returns None if LCU is not running
if let Some(creds) = try_find_lcu() {
    println!("port={} password={}", creds.port, creds.password);
}

// Blocking poll: waits until LCU launches (like awaitConnection: true)
let creds = authenticate(3000).await; // poll every 3 s
```

### 2 — REST requests

```rust
use league_connect_rust::{authenticate, build_lcu_client, lcu_get, parse_marketing_version};

let creds  = authenticate(3000).await;
let client = build_lcu_client(); // reuse — it manages a connection pool

// Any LCU endpoint; returns serde_json::Value or None on error/404
let ver = lcu_get(&client, &creds, "/lol-patch/v1/game-version").await;
if let Some(raw) = ver.and_then(|v| v.as_str().map(String::from)) {
    println!("patch {}", parse_marketing_version(&raw).unwrap()); // e.g. "14.21"
}

let session = lcu_get(&client, &creds, "/lol-gameflow/v1/session").await;
let phase   = session.as_ref().and_then(|v| v["phase"].as_str());
```

### 3 — WebSocket event subscription

```rust
use league_connect_rust::{authenticate, connect, EventType};

let creds = authenticate(3000).await;
let mut rx = connect(&creds, 64).await?; // 64 = channel buffer size

while let Some(event) = rx.recv().await {
    // event: LcuEvent { uri, event_type, data }
    match event.uri.as_str() {
        "/lol-champ-select/v1/session" => {
            if event.event_type == EventType::Delete {
                println!("session ended");
            } else {
                println!("phase: {}", event.data["timer"]["phase"]);
            }
        }
        _ => {}
    }
}
// rx.recv() returns None when the LCU WebSocket closes
```

### Run the demo

```bash
# Start League Client first, then:
cargo run --example connect
```

---

## API Reference

### `auth` module

```rust
pub struct Credentials {
    pub port:     u16,
    pub password: String,
    pub pid:      u32,
}

impl Credentials {
    pub fn basic_auth(&self)   -> String  // "Basic <base64>"
    pub fn lcu_base_url(&self) -> String  // "https://127.0.0.1:{port}"
    pub fn lcu_ws_url(&self)   -> String  // "wss://127.0.0.1:{port}"
}

pub fn  try_find_lcu()                     -> Option<Credentials>
pub async fn authenticate(poll_ms: u64)    -> Credentials
```

### `http` module

```rust
pub fn  build_lcu_client()                                      -> reqwest::Client
pub async fn lcu_get(client, creds, endpoint) -> Option<Value>
pub fn  parse_marketing_version(raw: &str)    -> Option<String>
```

### `websocket` module

```rust
pub struct LcuEvent {
    pub uri:        String,
    pub event_type: EventType,
    pub data:       serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType { Create, Update, Delete, Unknown }

pub async fn connect(creds: &Credentials, buffer: usize)
    -> Result<mpsc::Receiver<LcuEvent>, Box<dyn Error + Send + Sync>>
```

---

## Design notes

### Why `sysinfo` instead of PowerShell?

The JS library spawns a PowerShell child process (`Get-CimInstance`) on every `authenticate()` call — roughly 200–400 ms of overhead. `sysinfo` queries the Windows kernel directly (`NtQuerySystemInformation`), taking < 10 ms.

### Why `mpsc` channel instead of callbacks?

The JS `LeagueWebSocket` uses a `Map<uri, callbacks[]>`. The idiomatic Rust equivalent is an `mpsc` channel:

```
tokio task (owns WS stream)
    └── tx.send(event) ──channel──► caller: while let Some(e) = rx.recv().await
```

When the LCU disconnects, the task drops `tx`, and `rx.recv()` returns `None` — the caller naturally knows to reconnect without needing a separate signal.

### WAMP protocol

The LCU WebSocket uses a simplified subset of [WAMP](https://wamp-proto.org/):

```
Client → Server:  [5, "OnJsonApiEvent"]        // Subscribe (opcode 5)
Server → Client:  [8, "OnJsonApiEvent", {...}] // Event     (opcode 8)
                                  └── { uri, data, eventType }
```

---

## Relation to the original JS library

This library was extracted from [lol-bp-ui-rs](https://github.com/QAQTam/lol-bp-ui-rs), a Tauri rewrite of an Electron-based League of Legends tournament broadcast tool. The LCU connection logic was ported from the JS `league-connect` library source as part of that migration.

---

## License

MIT
