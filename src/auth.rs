use regex::Regex;
use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, System, UpdateKind};

/// 对应 JS Credentials 接口
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub port: u16,
    pub password: String,
    pub pid: u32,
}

/// 构建 Basic Auth header 值，供 HTTP 和 WebSocket 使用
/// 对应 JS: Buffer.from(`riot:${password}`).toString('base64')
impl Credentials {
    pub fn basic_auth(&self) -> String {
        use base64::{engine::general_purpose, Engine as _};
        let raw = format!("riot:{}", self.password);
        format!("Basic {}", general_purpose::STANDARD.encode(raw))
    }

    pub fn lcu_base_url(&self) -> String {
        format!("https://127.0.0.1:{}", self.port)
    }

    pub fn lcu_ws_url(&self) -> String {
        format!("wss://127.0.0.1:{}", self.port)
    }
}

/// 尝试一次进程查找。
///
/// 对应 JS authenticate() 的核心逻辑：
///   Get-CimInstance ... WHERE name LIKE 'LeagueClientUx.exe'
///   + regex 提取 --app-port / --remoting-auth-token / --app-pid
///
/// 返回 None 表示 LCU 未运行（而非 panic），调用方决定是否重试。
pub fn try_find_lcu() -> Option<Credentials> {
    // 只刷新进程命令行，减少内存和 CPU 开销
    // sysinfo 0.32 API: refresh_processes_specifics 需要明确 UpdateKind
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
    );

    // 预编译正则（实际使用时应放到 lazy_static/OnceLock 中）
    let port_re = Regex::new(r"--app-port=(\d+)").unwrap();
    let pass_re = Regex::new(r"--remoting-auth-token=([\w-]+)").unwrap();

    for (pid, process) in sys.processes() {
        // process.name() 在 Windows 上返回不含路径的可执行文件名
        let name = process.name().to_string_lossy();
        if !name.contains("LeagueClientUx") {
            continue;
        }

        // cmd() 返回 &[OsString]，每个元素是一个参数
        // 拼成一个字符串方便统一 regex 匹配
        let cmdline: String = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");

        // 任一字段缺失则跳过（进程可能正在启动中，参数还不完整）
        let port: u16 = port_re
            .captures(&cmdline)?
            .get(1)?
            .as_str()
            .parse()
            .ok()?;

        let password = pass_re
            .captures(&cmdline)?
            .get(1)?
            .as_str()
            .to_string();

        return Some(Credentials {
            port,
            password,
            pid: pid.as_u32(),
        });
    }

    None
}

/// 轮询直到找到 LCU，对应 JS authenticate({ awaitConnection: true })
///
/// 永不返回 Err，找到即 return。
pub async fn authenticate(poll_interval_ms: u64) -> Credentials {
    loop {
        if let Some(creds) = try_find_lcu() {
            return creds;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(poll_interval_ms)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// cargo test -- --nocapture
    /// 需要 LeagueClientUx.exe 正在运行
    #[test]
    fn test_find_lcu_running() {
        match try_find_lcu() {
            Some(creds) => {
                println!("Found LCU:");
                println!("  PID:      {}", creds.pid);
                println!("  Port:     {}", creds.port);
                println!("  Password: {}", creds.password);
                println!("  Auth:     {}", creds.basic_auth());
                println!("  WS URL:   {}", creds.lcu_ws_url());
                assert!(creds.port > 0);
                assert!(!creds.password.is_empty());
            }
            None => {
                println!("LCU not running — skipping (not a failure)");
            }
        }
    }
}
