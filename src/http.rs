use reqwest::{Client, ClientBuilder, Method};
use serde_json::Value;

use super::auth::Credentials;

/// Build a reusable HTTP client configured for the LCU API.
///
/// The League Client uses a Riot Games self-signed certificate.
/// This client skips TLS verification so requests are not rejected.
///
/// **Reuse this client** — it maintains an internal connection pool.
/// Creating one per request wastes TLS handshakes.
pub fn build_lcu_client() -> Client {
    ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .expect("failed to build LCU HTTP client")
}

// ─── Generic request ─────────────────────────────────────────

/// Send any HTTP request to the LCU API.
///
/// Returns `None` on network error, non-2xx status, or invalid JSON body.
/// This is the low-level building block — prefer the convenience wrappers
/// [`lcu_get`], [`lcu_post`], [`lcu_put`], [`lcu_delete`] for common cases.
pub async fn lcu_request(
    client: &Client,
    credentials: &Credentials,
    method: Method,
    endpoint: &str,
    body: Option<&Value>,
) -> Option<Value> {
    let url = format!("{}{}", credentials.lcu_base_url(), endpoint);

    let mut req = client
        .request(method, &url)
        .header("Authorization", credentials.basic_auth())
        .header("Accept", "application/json");

    if let Some(json_body) = body {
        req = req.json(json_body);
    }

    req.send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<Value>()
        .await
        .ok()
}

// ─── Convenience wrappers ────────────────────────────────────

/// `GET` an LCU endpoint. Returns `None` on error or non-2xx.
///
/// ```no_run
/// # async fn example(client: &reqwest::Client, creds: &league_connect_rust::Credentials) {
/// // Get current summoner info
/// let me = league_connect_rust::lcu_get(client, creds, "/lol-summoner/v1/current-summoner").await;
///
/// // Get lobby members
/// let lobby = league_connect_rust::lcu_get(client, creds, "/lol-lobby/v2/lobby").await;
///
/// // Get game version
/// let ver = league_connect_rust::lcu_get(client, creds, "/lol-patch/v1/game-version").await;
/// # }
/// ```
pub async fn lcu_get(client: &Client, credentials: &Credentials, endpoint: &str) -> Option<Value> {
    lcu_request(client, credentials, Method::GET, endpoint, None).await
}

/// `POST` to an LCU endpoint with a JSON body.
///
/// ```no_run
/// # async fn example(client: &reqwest::Client, creds: &league_connect_rust::Credentials) {
/// // Create a lobby
/// let body = serde_json::json!({ "queueId": 420 });
/// league_connect_rust::lcu_post(client, creds, "/lol-lobby/v2/lobby", &body).await;
///
/// // Accept a ready check
/// league_connect_rust::lcu_post(client, creds, "/lol-matchmaking/v1/ready-check/accept", &serde_json::json!({})).await;
/// # }
/// ```
pub async fn lcu_post(
    client: &Client,
    credentials: &Credentials,
    endpoint: &str,
    body: &Value,
) -> Option<Value> {
    lcu_request(client, credentials, Method::POST, endpoint, Some(body)).await
}

/// `PUT` to an LCU endpoint with a JSON body.
///
/// ```no_run
/// # async fn example(client: &reqwest::Client, creds: &league_connect_rust::Credentials) {
/// // Lock in a champion during champ select
/// let body = serde_json::json!({ "championId": 1, "completed": true });
/// league_connect_rust::lcu_put(client, creds, "/lol-champ-select/v1/session/actions/1", &body).await;
/// # }
/// ```
pub async fn lcu_put(
    client: &Client,
    credentials: &Credentials,
    endpoint: &str,
    body: &Value,
) -> Option<Value> {
    lcu_request(client, credentials, Method::PUT, endpoint, Some(body)).await
}

/// `DELETE` an LCU endpoint. Returns `None` on error or non-2xx.
///
/// ```no_run
/// # async fn example(client: &reqwest::Client, creds: &league_connect_rust::Credentials) {
/// // Leave the current lobby
/// league_connect_rust::lcu_delete(client, creds, "/lol-lobby/v2/lobby").await;
/// # }
/// ```
pub async fn lcu_delete(
    client: &Client,
    credentials: &Credentials,
    endpoint: &str,
) -> Option<Value> {
    lcu_request(client, credentials, Method::DELETE, endpoint, None).await
}

/// `PATCH` an LCU endpoint with a JSON body.
pub async fn lcu_patch(
    client: &Client,
    credentials: &Credentials,
    endpoint: &str,
    body: &Value,
) -> Option<Value> {
    lcu_request(client, credentials, Method::PATCH, endpoint, Some(body)).await
}

// ─── Utility ─────────────────────────────────────────────────

/// Convert an internal LCU version string to the player-facing patch number.
///
/// The LCU reports versions like `"4.21.614.6789"` where `4` is the internal
/// season offset (Season 4 = 0). Adding 10 yields the marketing version:
/// `"14.21"`.
///
/// Returns `None` if the input is not in the expected format.
pub fn parse_marketing_version(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let internal_major: u32 = parts[0].parse().ok()?;
    let minor = parts[1];
    Some(format!("{}.{:0>2}", internal_major + 10, minor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::try_find_lcu;

    #[test]
    fn test_version_parsing() {
        assert_eq!(parse_marketing_version("4.21.614.6789"), Some("14.21".into()));
        assert_eq!(parse_marketing_version("4.3.614.6789"), Some("14.03".into()));
        assert_eq!(parse_marketing_version("bad"), None);
    }

    #[tokio::test]
    #[ignore]
    async fn test_lcu_get_summoner() {
        let creds = try_find_lcu().expect("LCU not running");
        let client = build_lcu_client();
        let me = lcu_get(&client, &creds, "/lol-summoner/v1/current-summoner").await;
        println!("current summoner: {:?}", me);
        assert!(me.is_some());
    }

    #[tokio::test]
    #[ignore]
    async fn test_lcu_get_version() {
        let creds = try_find_lcu().expect("LCU not running");
        let client = build_lcu_client();
        let ver = lcu_get(&client, &creds, "/lol-patch/v1/game-version")
            .await
            .expect("request failed");
        let raw = ver.as_str().expect("expected a string");
        println!("raw={raw}  marketing={}", parse_marketing_version(raw).unwrap_or_default());
    }
}
