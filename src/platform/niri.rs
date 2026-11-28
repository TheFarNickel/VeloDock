use crate::platform::tracker::{WindowInfo, WindowTracker, WorkspaceInfo};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

pub fn parse_niri_windows(windows_json: &str, active_ws_id: Option<u64>) -> Vec<WindowInfo> {
    let Ok(root) = serde_json::from_str::<Value>(windows_json) else {
        return Vec::new();
    };
    let Some(arr) = root
        .get("Ok")
        .and_then(|ok| ok.get("Windows"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|w| {
            let id = w.get("id")?.as_u64()?.to_string();
            let app_id = w
                .get("app_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())?
                .to_string();
            let title = w
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let focused = w
                .get("is_focused")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let ws_id = w.get("workspace_id").and_then(Value::as_u64);
            let on_active_workspace = match (active_ws_id, ws_id) {
                (Some(cur), Some(wid)) => cur == wid,
                _ => true,
            };
            Some(WindowInfo {
                id,
                app_id,
                title,
                focused,
                on_active_workspace,
                geometry: None,
            })
        })
        .collect()
}

pub fn parse_niri_active_workspace(workspaces_json: &str) -> Option<u64> {
    let root = serde_json::from_str::<Value>(workspaces_json).ok()?;
    let arr = root
        .get("Ok")
        .and_then(|ok| ok.get("Workspaces"))
        .and_then(Value::as_array)?;
    arr.iter().find_map(|ws| {
        if ws.get("is_focused").and_then(Value::as_bool).unwrap_or(false)
            || ws.get("is_active").and_then(Value::as_bool).unwrap_or(false)
        {
            ws.get("id").and_then(Value::as_u64)
        } else {
            None
        }
    })
}

pub struct NiriTracker {
    socket: PathBuf,
}

impl NiriTracker {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("NIRI_SOCKET")
            .ok()
            .filter(|s| !s.is_empty())?;
        Some(Self {
            socket: PathBuf::from(path),
        })
    }

    fn request(&self, payload: &str) -> Option<String> {
        let mut stream = UnixStream::connect(&self.socket).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        stream.write_all(payload.as_bytes()).ok()?;
        stream.write_all(b"\n").ok()?;
        let _ = stream.flush();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        Some(line)
    }
}

impl WindowTracker for NiriTracker {
    fn snapshot(&self) -> Vec<WindowInfo> {
        let active_ws = self
            .request("\"Workspaces\"")
            .and_then(|j| parse_niri_active_workspace(&j));
        self.request("\"Windows\"")
            .map(|j| parse_niri_windows(&j, active_ws))
            .unwrap_or_default()
    }

    fn focus_window(&self, id: &str) {
        if let Ok(num_id) = id.parse::<u64>() {
            let _ = self.request(&format!(
                "{{\"Action\":{{\"FocusWindow\":{{\"id\":{num_id}}}}}}}"
            ));
        }
    }

    fn close_window(&self, id: &str) {
        if let Ok(num_id) = id.parse::<u64>() {
            let _ = self.request(&format!(
                "{{\"Action\":{{\"CloseWindow\":{{\"id\":{num_id}}}}}}}"
            ));
        }
    }

    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        let Some(raw) = self.request("\"Workspaces\"") else {
            return Vec::new();
        };
        let Ok(root) = serde_json::from_str::<Value>(&raw) else {
            return Vec::new();
        };
        let Some(arr) = root
            .get("Ok")
            .and_then(|ok| ok.get("Workspaces"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        let mut out: Vec<(u64, WorkspaceInfo)> = arr
            .iter()
            .filter_map(|ws| {
                let idx = ws.get("idx").and_then(Value::as_u64).unwrap_or(0);
                let id = ws.get("id")?.as_u64()?;
                let name = ws
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| if idx > 0 { idx.to_string() } else { id.to_string() });
                let active = ws.get("is_focused").and_then(Value::as_bool).unwrap_or(false)
                    || ws.get("is_active").and_then(Value::as_bool).unwrap_or(false);
                Some((
                    idx,
                    WorkspaceInfo {
                        id: id.to_string(),
                        name,
                        active,
                    },
                ))
            })
            .collect();
        out.sort_by_key(|(idx, _)| *idx);
        out.into_iter().map(|(_, w)| w).collect()
    }

    fn switch_workspace(&self, id: &str) {
        if let Ok(num_id) = id.parse::<u64>() {
            let _ = self.request(&format!(
                "{{\"Action\":{{\"FocusWorkspace\":{{\"reference\":{{\"Id\":{num_id}}}}}}}}}"
            ));
        }
    }

    fn start_listener(&self, tx: async_channel::Sender<Vec<WindowInfo>>) {
        let socket = self.socket.clone();
        std::thread::spawn(move || {
            let tracker = NiriTracker { socket };
            let _ = tx.send_blocking(tracker.snapshot());
            let mut backoff_ms = 250_u64;

            loop {
                match UnixStream::connect(&tracker.socket) {
                    Ok(mut stream) => {
                        backoff_ms = 250;
                        if stream.write_all(b"\"EventStream\"\n").is_ok()
                            && stream.flush().is_ok()
                        {
                            let reader = BufReader::new(stream);
                            for line in reader.lines().map_while(Result::ok) {
                                let relevant = line.contains("Window")
                                    || line.contains("Workspace");
                                if relevant && tx.send_blocking(tracker.snapshot()).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(backoff_ms));
                        backoff_ms = (backoff_ms * 2).min(4000);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_niri_windows_and_active_workspace() {
        let ws_json = r#"{"Ok":{"Workspaces":[{"id":10,"is_active":true,"is_focused":true},{"id":11,"is_active":false,"is_focused":false}]}}"#;
        let active = parse_niri_active_workspace(ws_json);
        assert_eq!(active, Some(10));

        let win_json = r#"{"Ok":{"Windows":[{"id":1,"title":"Term","app_id":"kitty","workspace_id":10,"is_focused":true},{"id":2,"title":"Web","app_id":"firefox","workspace_id":11,"is_focused":false}]}}"#;
        let wins = parse_niri_windows(win_json, active);
        assert_eq!(wins.len(), 2);
        assert_eq!(wins[0].id, "1");
        assert!(wins[0].focused);
        assert!(wins[0].on_active_workspace);
        assert!(!wins[1].on_active_workspace);
    }
}
