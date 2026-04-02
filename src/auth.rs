use regex::Regex;
use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, System, UpdateKind};

/// LCU API credentials extracted from the running League Client process.
///
/// The League Client passes `--app-port` and `--remoting-auth-token` as
/// command-line arguments. This struct holds those values along with the
/// system PID, and provides helpers to build auth headers and base URLs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    /// The local port the LCU API is listening on
    pub port: u16,
    /// The auth token for HTTP Basic authentication (username is always `riot`)
    pub password: String,
    /// System process ID of `LeagueClientUx`
    pub pid: u32,
}

impl Credentials {
    /// Build the `Authorization: Basic ...` header value.
    ///
    /// The LCU API uses HTTP Basic Auth with username `riot` and the
    /// remoting auth token as the password.
    pub fn basic_auth(&self) -> String {
        use base64::{engine::general_purpose, Engine as _};
        let raw = format!("riot:{}", self.password);
        format!("Basic {}", general_purpose::STANDARD.encode(raw))
    }

    /// HTTPS base URL, e.g. `https://127.0.0.1:52437`
    pub fn lcu_base_url(&self) -> String {
        format!("https://127.0.0.1:{}", self.port)
    }

    /// WSS URL for the LCU WebSocket, e.g. `wss://127.0.0.1:52437`
    pub fn lcu_ws_url(&self) -> String {
        format!("wss://127.0.0.1:{}", self.port)
    }
}

/// Attempt to find a running `LeagueClientUx` process **once**.
///
/// Returns `None` if the process is not found or if the command-line
/// arguments haven't fully appeared yet (the process may still be starting).
///
/// Uses the `sysinfo` crate to enumerate processes directly through the OS
/// kernel API — no shell subprocess is spawned.
pub fn try_find_lcu() -> Option<Credentials> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
    );

    let port_re = Regex::new(r"--app-port=(\d+)").unwrap();
    let pass_re = Regex::new(r"--remoting-auth-token=([\w-]+)").unwrap();

    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy();
        if !name.contains("LeagueClientUx") {
            continue;
        }

        let cmdline: String = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");

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

/// Poll until a running League Client is found.
///
/// Calls [`try_find_lcu`] in a loop, sleeping `poll_interval_ms` between
/// attempts. Never returns `Err` — it simply waits.
///
/// ```no_run
/// # async fn example() {
/// let creds = league_connect_rust::authenticate(3000).await; // poll every 3 s
/// println!("LCU found on port {}", creds.port);
/// # }
/// ```
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

    #[test]
    fn test_find_lcu_running() {
        match try_find_lcu() {
            Some(creds) => {
                println!("Found LCU:");
                println!("  PID:      {}", creds.pid);
                println!("  Port:     {}", creds.port);
                println!("  Auth:     {}", creds.basic_auth());
                assert!(creds.port > 0);
                assert!(!creds.password.is_empty());
            }
            None => {
                println!("LCU not running — skipping (not a failure)");
            }
        }
    }
}
