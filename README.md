# league-connect-rust

Rust client for the **League of Legends Client Update (LCU) API**.

A Rust port of [league-connect](https://github.com/matsjla/league-connect) (Node.js), providing:

- **Process discovery** — find the running League Client and extract API credentials
- **REST client** — `GET` / `POST` / `PUT` / `DELETE` / `PATCH` any LCU endpoint
- **WebSocket (WAMP)** — subscribe to all real-time LCU events via `tokio::sync::mpsc`

| Feature | JS original | Rust implementation |
|---|---|---|
| Process discovery | PowerShell `Get-CimInstance` (~200-400ms) | `sysinfo` crate — direct kernel API (~10ms) |
| REST requests | `https.request` + `rejectUnauthorized: false` | `reqwest` + `native-tls` |
| WebSocket | `ws` + callback Map | `tokio-tungstenite` + `mpsc` channel |

---

## Quick Start

```toml
[dependencies]
league-connect-rust = { git = "https://github.com/QAQTam/league-connect-rust" }
tokio = { version = "1", features = ["full"] }
```

### Process Discovery

```rust
use league_connect_rust::{try_find_lcu, authenticate};

// One-shot: returns None if LCU is not running
if let Some(creds) = try_find_lcu() {
    println!("port={} pid={}", creds.port, creds.pid);
}

// Poll until LCU launches
let creds = authenticate(3000).await; // check every 3s
```

### REST API

```rust
use league_connect_rust::{authenticate, build_lcu_client, lcu_get, lcu_post};

let creds  = authenticate(3000).await;
let client = build_lcu_client(); // reuse — maintains connection pool

// GET any LCU endpoint
let me = lcu_get(&client, &creds, "/lol-summoner/v1/current-summoner").await;

// POST with JSON body
let body = serde_json::json!({ "queueId": 420 });
lcu_post(&client, &creds, "/lol-lobby/v2/lobby", &body).await;
```

Available: `lcu_get`, `lcu_post`, `lcu_put`, `lcu_delete`, `lcu_patch`, and the generic `lcu_request` for full control.

### WebSocket Events

```rust
use league_connect_rust::{authenticate, connect, EventType};

let creds = authenticate(3000).await;
let mut rx = connect(&creds, 128).await?;

while let Some(event) = rx.recv().await {
    // event.uri       — e.g. "/lol-gameflow/v1/session"
    // event.event_type — Create, Update, Delete
    // event.data       — serde_json::Value
    println!("[{:?}] {}", event.event_type, event.uri);
}
// rx.recv() returns None when LCU disconnects
```

### Run the Examples

```bash
# WebSocket event monitor — prints ALL LCU events
cargo run --example connect

# REST API demo — summoner info, game version, gameflow phase
cargo run --example rest
```

---

## API Reference

### `auth` module

| Function | Description |
|---|---|
| `try_find_lcu() -> Option<Credentials>` | One-shot process scan |
| `authenticate(poll_ms) -> Credentials` | Poll until LCU is found |

`Credentials` provides: `basic_auth()`, `lcu_base_url()`, `lcu_ws_url()`

### `http` module

| Function | Description |
|---|---|
| `build_lcu_client() -> Client` | Reusable HTTP client (skips self-signed cert verification) |
| `lcu_request(client, creds, method, endpoint, body)` | Generic request |
| `lcu_get` / `lcu_post` / `lcu_put` / `lcu_delete` / `lcu_patch` | Convenience wrappers |
| `parse_marketing_version(raw) -> Option<String>` | `"4.21.614.6789"` -> `"14.21"` |

### `websocket` module

| Function / Type | Description |
|---|---|
| `connect(creds, buffer) -> Result<Receiver<LcuEvent>>` | Subscribe to all LCU events |
| `LcuEvent { uri, event_type, data }` | A single event |
| `EventType { Create, Update, Delete, Unknown }` | Event classification |

---

## Design Notes

### Why `sysinfo` instead of PowerShell?

The JS library spawns a PowerShell child process on every poll — ~200-400ms overhead. `sysinfo` queries `NtQuerySystemInformation` directly, taking <10ms.

### Why `mpsc` channel instead of callbacks?

```
tokio task (owns WebSocket)
    └── tx.send(event) ──channel──► caller: rx.recv().await
```

When the LCU disconnects, the task drops `tx`, and `rx.recv()` returns `None` — the caller naturally knows the connection is gone. No separate close signals, no callback cleanup.

### WAMP Protocol

The LCU uses a simplified WAMP subset:

```
Client → Server:  [5, "OnJsonApiEvent"]          // Subscribe
Server → Client:  [8, "OnJsonApiEvent", {payload}] // Event
                                  └── { uri, data, eventType }
```

---

## AI Authorship

This library was written with AI assistance. The code, architecture, documentation, and examples were authored collaboratively by:

- **[Claude](https://claude.ai)** (Anthropic) — primary code author
- **[Gemini](https://gemini.google.com)** (Google) — contributed to the original project

All in AI. All love AI.

The library was extracted from [lol-bp-ui-rs](https://github.com/QAQTam/league-banpickUI-rs), a Tauri rewrite of an Electron-based League of Legends broadcast tool. Human direction and vision by [@QAQTam](https://github.com/QAQTam).

---

## License

MIT
