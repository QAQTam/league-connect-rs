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

// ─── 公共类型 ────────────────────────────────────────────────────────────────

/// 解析后的 LCU WebSocket 事件。
///
/// 对应 JS EventResponse<T> + eventType 字段：
///   { uri: string, data: T, eventType: 'Create'|'Update'|'Delete' }
#[derive(Debug, Clone)]
pub struct LcuEvent {
    pub uri: String,
    pub event_type: EventType,
    pub data: Value,
}

/// LCU 事件类型，对应 WAMP payload 里的 eventType 字段。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum EventType {
    Create,
    Update,
    Delete,
    #[serde(other)]
    Unknown,
}

// ─── 内部 WAMP 反序列化结构 ──────────────────────────────────────────────────

/// LCU 推送格式（WAMP Event 帧，opcode 8）：
///   [8, "OnJsonApiEvent", { uri, data, eventType }]
///
/// json.slice(2) 对应 Rust 这里取 arr[2]。
#[derive(Debug, Deserialize)]
struct WampPayload {
    uri: String,
    data: Value,
    #[serde(rename = "eventType")]
    event_type: EventType,
}

// ─── 核心连接函数 ─────────────────────────────────────────────────────────────

/// 连接到 LCU WebSocket，返回事件接收端。
///
/// # 设计说明（与 JS 的差异）
///
/// JS 版本：LeagueWebSocket 继承 WebSocket，用 Map<uri, callbacks[]> 在回调中分发。
///
/// Rust 版本：用 mpsc channel 代替回调 Map。
///   - 函数内部 spawn 一个 tokio task 持有 WS 连接
///   - 所有事件通过 Sender<LcuEvent> 发出
///   - 调用方持有 Receiver，可按需过滤 uri，或转发给 Tauri emit()
///
/// 优势：
///   - 避免 Box<dyn Fn> 带来的生命周期麻烦
///   - Receiver 可以在多个 await 点上自然地 .recv()
///   - task 结束（LCU 断开）时 Sender drop，Receiver.recv() 返回 None，
///     调用方可据此触发重连
///
/// # 参数
/// - `credentials`: 通过 auth::authenticate() 获取
/// - `buffer`: mpsc channel 缓冲大小，建议 64～256（防止 LCU 事件突发时丢帧）
///
/// # 返回
/// - `Ok(Receiver<LcuEvent>)`: 连接成功，可开始 recv()
/// - `Err(...)`: TLS 构建失败或 WebSocket 握手失败
pub async fn connect(
    credentials: &Credentials,
    buffer: usize,
) -> Result<mpsc::Receiver<LcuEvent>, Box<dyn std::error::Error + Send + Sync>> {
    // 1. 构建跳过证书验证的 TLS connector
    //    对应 JS: new https.Agent({ rejectUnauthorized: false })
    let tls = TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()?;

    // 2. 构建 WebSocket 升级请求，注入 Basic Auth header
    //    对应 JS: headers: { Authorization: 'Basic ' + Buffer.from(...).toString('base64') }
    let mut request = credentials.lcu_ws_url().into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&credentials.basic_auth())?,
    );

    // 3. 建立 WSS 连接
    let (mut ws_stream, _response) = connect_async_tls_with_config(
        request,
        None,  // 默认 WebSocketConfig
        false, // disable_nagle: false（Nagle 算法对小帧友好）
        Some(Connector::NativeTls(tls)),
    )
    .await?;

    // 4. 发送 WAMP Subscribe 帧
    //    对应 JS: this.send(JSON.stringify([5, 'OnJsonApiEvent']))
    //    WAMP opcode 5 = SUBSCRIBE，订阅所有 JSON API 事件
    ws_stream
        .send(Message::Text(
            serde_json::json!([5, "OnJsonApiEvent"]).to_string().into(),
        ))
        .await?;

    // 5. 建立 channel，spawn 后台读循环
    let (tx, rx) = mpsc::channel::<LcuEvent>(buffer);

    tokio::spawn(async move {
        while let Some(msg) = ws_stream.next().await {
            let text = match msg {
                Ok(Message::Text(t)) => t,
                Ok(Message::Close(_)) | Err(_) => break, // 正常断开或网络错误，退出循环
                _ => continue,                            // Ping/Pong/Binary 帧，忽略
            };

            // 6. 解析 WAMP Event 帧：[8, "OnJsonApiEvent", payload]
            //    对应 JS: const json = JSON.parse(content); const [res] = json.slice(2)
            if let Some(event) = parse_wamp_event(&text) {
                // Sender::send 失败说明 Receiver 已 drop（调用方不再监听），退出
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        }
        // tx 在此 drop → Receiver::recv() 返回 None → 调用方知道连接已断
    });

    Ok(rx)
}

// ─── 内部辅助 ─────────────────────────────────────────────────────────────────

/// 解析一条原始 WebSocket 文本帧为 LcuEvent。
///
/// 格式：[opcode, topic, payload]
///   opcode 8 = WAMP EVENT
///   payload  = { uri, data, eventType }
fn parse_wamp_event(text: &str) -> Option<LcuEvent> {
    let arr: Vec<Value> = serde_json::from_str(text).ok()?;

    // 必须是 3 个元素，且第一个是 8（WAMP EVENT opcode）
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

// ─── 测试 ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wamp_event_valid() {
        // 模拟一条真实的 LCU champ-select 事件帧
        let raw = r#"[8,"OnJsonApiEvent",{"uri":"/lol-champ-select/v1/session","eventType":"Update","data":{"timer":{"phase":"BAN_PICK"}}}]"#;
        let event = parse_wamp_event(raw).unwrap();

        assert_eq!(event.uri, "/lol-champ-select/v1/session");
        assert_eq!(event.event_type, EventType::Update);
        assert!(event.data["timer"]["phase"].as_str() == Some("BAN_PICK"));
    }

    #[test]
    fn test_parse_wamp_event_delete() {
        let raw = r#"[8,"OnJsonApiEvent",{"uri":"/lol-champ-select/v1/session","eventType":"Delete","data":null}]"#;
        let event = parse_wamp_event(raw).unwrap();
        assert_eq!(event.event_type, EventType::Delete);
    }

    #[test]
    fn test_parse_wamp_event_ignores_non_event_frames() {
        // opcode 5 是 Subscribe 确认帧，不应被解析为事件
        let raw = r#"[5,"OnJsonApiEvent"]"#;
        assert!(parse_wamp_event(raw).is_none());

        // 格式错误
        assert!(parse_wamp_event("not json").is_none());
        assert!(parse_wamp_event("[]").is_none());
    }

    /// 需要 LCU 运行时执行：cargo test -- --nocapture --ignored
    #[tokio::test]
    #[ignore]
    async fn test_connect_and_receive() {
        use crate::auth::try_find_lcu;

        let creds = try_find_lcu().expect("LCU not running");
        let mut rx = connect(&creds, 64).await.expect("WS connect failed");

        println!("Connected. Waiting for events (进入英雄选择触发)...");
        while let Some(event) = rx.recv().await {
            println!(
                "[{}] {} → {}",
                match event.event_type {
                    EventType::Create => "CREATE",
                    EventType::Update => "UPDATE",
                    EventType::Delete => "DELETE",
                    EventType::Unknown => "?",
                },
                event.uri,
                event.data
            );
        }
        println!("Connection closed.");
    }
}
