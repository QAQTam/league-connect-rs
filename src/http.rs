use reqwest::{Client, ClientBuilder};
use serde_json::Value;

use super::auth::Credentials;

/// 构建一个可复用的 LCU HTTP Client。
///
/// 对应 JS 中每次 lcuRequest() 都隐式创建的 https.Agent：
///   { rejectUnauthorized: false }
///
/// 注意：Client 内部维护连接池，应当全局只建一次（见 main.rs 的 AppState），
/// 而不是每次请求都 build。
pub fn build_lcu_client() -> Client {
    ClientBuilder::new()
        // 等价 rejectUnauthorized: false —— LCU 使用 Riot 自签名证书
        .danger_accept_invalid_certs(true)
        // LCU 有时返回非标准 hostname，一并跳过
        .danger_accept_invalid_hostnames(true)
        .build()
        .expect("failed to build LCU HTTP client")
}

/// 向 LCU REST API 发送一次 GET 请求，返回解析后的 JSON。
///
/// 对应 JS lcuRequest(credentials, endpoint)：
///   https.request({ hostname: '127.0.0.1', port, path: endpoint,
///                   headers: { Authorization: 'Basic ...' },
///                   rejectUnauthorized: false })
///
/// 返回 None 的情况：
///   - 网络错误 / LCU 未就绪
///   - HTTP 非 2xx
///   - 响应体不是合法 JSON
pub async fn lcu_get(client: &Client, credentials: &Credentials, endpoint: &str) -> Option<Value> {
    let url = format!("{}{}", credentials.lcu_base_url(), endpoint);

    client
        .get(&url)
        .header("Authorization", credentials.basic_auth())
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?
        // 非 2xx 视为失败（LCU 在 session 不存在时返回 404）
        .error_for_status()
        .ok()?
        .json::<Value>()
        .await
        .ok()
}

/// 专门处理 /lol-patch/v1/game-version 的版本转换。
///
/// LCU 返回内部版本号，例如 "4.21.14.6789"
/// index.mjs 中对应的转换逻辑：
///   const parts = rawVersion.split('.')
///   `${parseInt(parts[0]) + 10}.${parts[1].padStart(2, '0')}`
/// → 营销版本号 "14.21"（加 10 是因为 Riot 内部版本从 S4 算起）
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
        // 典型的 LCU 内部版本格式
        assert_eq!(
            parse_marketing_version("4.21.614.6789"),
            Some("14.21".to_string())
        );
        assert_eq!(
            parse_marketing_version("4.03.614.6789"),
            Some("14.03".to_string())
        );
        // padStart(2, '0') 效果：个位数 minor 补零
        assert_eq!(
            parse_marketing_version("4.3.614.6789"),
            Some("14.03".to_string())  // "3" → "03"
        );
        assert_eq!(parse_marketing_version("bad"), None);
    }

    /// 需要 LCU 运行时执行：cargo test -- --nocapture --ignored
    #[tokio::test]
    #[ignore]
    async fn test_lcu_get_version() {
        let creds = try_find_lcu().expect("LCU not running");
        let client = build_lcu_client();

        let version = lcu_get(&client, &creds, "/lol-patch/v1/game-version")
            .await
            .expect("request failed");

        let raw = version.as_str().expect("expected a string value");
        println!("Raw version:       {}", raw);
        println!(
            "Marketing version: {}",
            parse_marketing_version(raw).unwrap_or_default()
        );
        assert!(!raw.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_lcu_get_gameflow() {
        let creds = try_find_lcu().expect("LCU not running");
        let client = build_lcu_client();

        // 在英雄选择阶段才有数据，平时返回 404 → None
        let session = lcu_get(&client, &creds, "/lol-gameflow/v1/session").await;
        match session {
            Some(val) => {
                let mode = &val["gameData"]["queue"]["gameMode"];
                println!("Game mode: {}", mode);
            }
            None => println!("Not in a game session (expected outside of champ-select)"),
        }
    }
}
