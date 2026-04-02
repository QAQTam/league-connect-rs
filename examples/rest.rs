//! REST API demo: summoner info, game version, lobby creation
//!
//! Run with: `cargo run --example rest`
//! (requires a running League Client)

use league_connect_rust::{
    authenticate, build_lcu_client, lcu_get, lcu_post, parse_marketing_version, try_find_lcu,
};

#[tokio::main]
async fn main() {
    println!("league-connect-rust — REST API demo\n");

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

    let client = build_lcu_client();

    // ── Game version ────────────────────────────────────────
    if let Some(ver) = lcu_get(&client, &creds, "/lol-patch/v1/game-version").await {
        let raw = ver.as_str().unwrap_or("unknown");
        let marketing = parse_marketing_version(raw).unwrap_or_else(|| raw.to_string());
        println!("[info] Game version: {raw}  (patch {marketing})");
    }

    // ── Current summoner ────────────────────────────────────
    if let Some(me) = lcu_get(&client, &creds, "/lol-summoner/v1/current-summoner").await {
        println!(
            "[info] Summoner: {}  level={}",
            me["gameName"].as_str().unwrap_or("?"),
            me["summonerLevel"].as_u64().unwrap_or(0),
        );
    }

    // ── Gameflow phase ──────────────────────────────────────
    if let Some(session) = lcu_get(&client, &creds, "/lol-gameflow/v1/session").await {
        println!(
            "[info] Gameflow phase: {}",
            session["phase"].as_str().unwrap_or("None"),
        );
    }

    // ── Create a ranked solo lobby (example — uncomment to use)
    // let body = serde_json::json!({ "queueId": 420 });
    // let result = lcu_post(&client, &creds, "/lol-lobby/v2/lobby", &body).await;
    // println!("[lobby] {:?}", result);

    println!("\nDone.");
}
