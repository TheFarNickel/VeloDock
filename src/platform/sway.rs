use crate::platform::tracker::{Rect, WindowInfo, WindowTracker, WorkspaceInfo};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const SWAY_MAGIC: &[u8; 6] = b"i3-ipc";
const MSG_RUN_COMMAND: u32 = 0;
const MSG_GET_WORKSPACES: u32 = 1;
const MSG_SUBSCRIBE: u32 = 2;
const MSG_GET_TREE: u32 = 4;

fn sway_send(stream: &mut UnixStream, msg_type: u32, payload: &[u8]) -> std::io::Result<String> {
    stream.write_all(SWAY_MAGIC)?;
    stream.write_all(&(payload.len() as u32).to_ne_bytes())?;
    stream.write_all(&msg_type.to_ne_bytes())?;
    stream.write_all(payload)?;
    sway_read(stream)
}

fn sway_read(stream: &mut UnixStream) -> std::io::Result<String> {
    let mut header = [0u8; 14];
    stream.read_exact(&mut header)?;
    let len = u32::from_ne_bytes([header[6], header[7], header[8], header[9]]) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

pub fn parse_sway_tree(json: &str) -> Vec<WindowInfo> {
    let Ok(root) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_sway_nodes(&root, &mut out);
    out
}

fn collect_sway_nodes(node: &Value, out: &mut Vec<WindowInfo>) {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or("");
    let is_window = matches!(node_type, "con" | "floating_con") && node.get("pid").is_some();
    if is_window {
        let app_id = node
            .get("app_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                node.get("window_properties")
                    .and_then(|wp| wp.get("class"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("")
            .to_string();

        if !app_id.is_empty() {
            let id = node
                .get("id")
                .and_then(Value::as_i64)
                .map(|v| v.to_string())
                .unwrap_or_default();
            let title = node
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let focused = node.get("focused").and_then(Value::as_bool).unwrap_or(false);
            let geometry = node.get("rect").and_then(|r| {
                Some(Rect {
                    x: r.get("x")?.as_i64()? as i32,
                    y: r.get("y")?.as_i64()? as i32,
                    w: r.get("width")?.as_i64()? as i32,
                    h: r.get("height")?.as_i64()? as i32,
                })
            });
            out.push(WindowInfo {
                id,
                app_id,
                title,
                focused,
                on_active_workspace: node.get("visible").and_then(Value::as_bool).unwrap_or(true),
                geometry,
            });
        }
    }

    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(Value::as_array) {
            for child in children {
                collect_sway_nodes(child, out);
            }
        }
    }
}

pub struct SwayTracker {
    socket: PathBuf,
}

impl SwayTracker {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("SWAYSOCK")
            .or_else(|_| std::env::var("I3SOCK"))
            .ok()
            .filter(|s| !s.is_empty())?;
        Some(Self {
            socket: PathBuf::from(path),
        })
    }

    fn command(&self, msg_type: u32, payload: &str) -> Option<String> {
        let mut stream = UnixStream::connect(&self.socket).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        sway_send(&mut stream, msg_type, payload.as_bytes()).ok()
    }
}

impl WindowTracker for SwayTracker {
    fn snapshot(&self) -> Vec<WindowInfo> {
        self.command(MSG_GET_TREE, "")
            .map(|j| parse_sway_tree(&j))
            .unwrap_or_default()
    }

    fn focus_window(&self, id: &str) {
        let _ = self.command(MSG_RUN_COMMAND, &format!("[con_id={id}] focus"));
    }

    fn close_window(&self, id: &str) {
        let _ = self.command(MSG_RUN_COMMAND, &format!("[con_id={id}] kill"));
    }

    fn minimize_window(&self, id: &str) {
        let _ = self.command(MSG_RUN_COMMAND, &format!("[con_id={id}] move scratchpad"));
    }

    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        let Some(raw) = self.command(MSG_GET_WORKSPACES, "") else {
            return Vec::new();
        };
        let Ok(arr) = serde_json::from_str::<Vec<Value>>(&raw) else {
            return Vec::new();
        };
        arr.into_iter()
            .filter_map(|w| {
                let name = w.get("name")?.as_str()?.to_string();
                let active = w.get("focused").and_then(Value::as_bool).unwrap_or(false)
                    || w.get("visible").and_then(Value::as_bool).unwrap_or(false);
                Some(WorkspaceInfo {
                    id: name.clone(),
                    name,
                    active,
                })
            })
            .collect()
    }

    fn switch_workspace(&self, id: &str) {
        let _ = self.command(MSG_RUN_COMMAND, &format!("workspace \"{id}\""));
    }

    fn start_listener(&self, tx: async_channel::Sender<Vec<WindowInfo>>) {
        let socket = self.socket.clone();
        std::thread::spawn(move || {
            let tracker = SwayTracker { socket };
            let _ = tx.send_blocking(tracker.snapshot());
            loop {
                if let Ok(mut stream) = UnixStream::connect(&tracker.socket) {
                    if sway_send(&mut stream, MSG_SUBSCRIBE, b"[\"window\",\"workspace\"]").is_ok() {
                        while sway_read(&mut stream).is_ok() {
                            if tx.send_blocking(tracker.snapshot()).is_err() {
                                return;
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sway_tree_windows() {
        let json = r#"{"type":"root","nodes":[{"type":"con","id":42,"pid":999,"app_id":"kitty","name":"Term","focused":true,"rect":{"x":10,"y":20,"width":500,"height":400}}]}"#;
        let wins = parse_sway_tree(json);
        assert_eq!(wins.len(), 1);
        assert_eq!(wins[0].id, "42");
        assert_eq!(wins[0].app_id, "kitty");
        assert!(wins[0].focused);
    }
}
