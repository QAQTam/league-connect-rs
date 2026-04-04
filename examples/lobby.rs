use league_connect_rust::{authenticate, build_lcu_client, lcu_get, try_find_lcu};
use serde_json::Value;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    println!("=== League Custom Lobby Monitor (Powered by league-connect-rs) ===");

    // 1. 自动关联 LCU 进程并获取凭据
    let creds = match try_find_lcu() {
        Some(c) => c,
        None => {
            println!("[Auth] 正在等待英雄联盟客户端启动...");
            authenticate(2000).await // 每2秒轮询一次
        }
    };
    println!("[Auth] 已连接到 LCU: 端口 {}", creds.port);

    // 2. 初始化忽略证书验证的 HTTP 客户端
    let client = build_lcu_client();

    println!("[Monitor] 正在监控自定义房间状态...\n");

    loop {
        // 3. 调用 LCU API 获取当前大厅信息
        // 涉及的 API 节点: /lol-lobby/v2/lobby
        if let Some(lobby) = lcu_get(&client, &creds, "/lol-lobby/v2/lobby").await {
            parse_lobby_status(&lobby);
        } else {
            println!("[Status] 目前不在房间内。");
        }

        // 每隔 3 秒刷新一次
        sleep(Duration::from_secs(3)).await;
    }
}

fn parse_lobby_status(lobby: &Value) {
    // 检查是否为自定义模式
    let is_custom = lobby["gameConfig"]["isCustom"].as_bool().unwrap_or(false);
    
    if !is_custom {
        println!("[Status] 当前在普通大厅中，非自定义房间。");
        return;
    }

    let game_mode = lobby["gameConfig"]["gameMode"].as_str().unwrap_or("Unknown");
    println!(">> 自定义房间类型: {}", game_mode);

    // 读取成员情况
    if let Some(members) = lobby["members"].as_array() {
        println!(">> 当前成员数: {}", members.len());
        for (i, member) in members.iter().enumerate() {
            let name = member["summonerName"].as_str().unwrap_or("Unknown");
            let team = if member["teamId"].as_i64() == Some(100) { "蓝色方" } else { "红色方" };
            let is_leader = member["isOwner"].as_bool().unwrap_or(false);
            
            println!(
                "   [{}] {}{} - {}", 
                i + 1, 
                name, 
                if is_leader { " (房主)" } else { "" },
                team
            );
        }
    }
    println!("--------------------------------------");
}