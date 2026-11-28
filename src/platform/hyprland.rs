use crate::platform::tracker::{Rect, WindowInfo, WindowTracker, WorkspaceInfo};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Default, Deserialize)]
struct HyprWorkspaceRef {
    #[serde(default)]
    id: i32,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct HyprClient {
    address: String,
    #[serde(default)]
    class: String,
    #[serde(default, rename = "initialClass")]
    initial_class: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    at: [i32; 2],
    #[serde(default)]
    size: [i32; 2],
    #[serde(default)]
    workspace: HyprWorkspaceRef,
    #[serde(default, rename = "focusHistoryID")]
    focus_history_id: i32,
    #[serde(default = "default_true")]
    mapped: bool,
    #[serde(default)]
    hidden: bool,
}

fn default_true() -> bool {
    true
}

pub fn parse_hyprland_clients(json: &str) -> Vec<WindowInfo> {
    parse_hyprland_clients_with_workspace(json, None)
}

pub fn parse_hyprland_clients_with_workspace(
    json: &str,
    active_workspace_id: Option<i32>,
) -> Vec<WindowInfo> {
    let Ok(clients) = serde_json::from_str::<Vec<HyprClient>>(json) else {
        return Vec::new();
    };
    clients
        .into_iter()
        .filter(|c| c.mapped && !c.hidden)
        .filter_map(|c| {
            let app_id = if !c.class.is_empty() {
                c.class
            } else if !c.initial_class.is_empty() {
                c.initial_class
            } else {
                return None;
            };
            let geometry = if c.size[0] > 0 && c.size[1] > 0 {
                Some(Rect {
                    x: c.at[0],
                    y: c.at[1],
                    w: c.size[0],
                    h: c.size[1],
                })
            } else {
                None
            };
            let on_active_workspace = match active_workspace_id {
                Some(active_id) => c.workspace.id == active_id,
                None => true,
            };
            Some(WindowInfo {
                id: c.address,
                app_id,
                title: c.title,
                focused: c.focus_history_id == 0,
                on_active_workspace,
                geometry,
            })
        })
        .collect()
}

pub struct HyprlandTracker {
    signature: String,
}

impl HyprlandTracker {
    pub fn from_env() -> Option<Self> {
        let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .ok()
            .filter(|s| !s.is_empty())?;
        Some(Self { signature: sig })
    }

    fn socket_path(&self, name: &str) -> PathBuf {
        if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
            let candidate = PathBuf::from(runtime)
                .join("hypr")
                .join(&self.signature)
                .join(name);
            if candidate.exists() {
                return candidate;
            }
        }
        PathBuf::from("/tmp/hypr").join(&self.signature).join(name)
    }

    fn request(&self, payload: &str) -> Option<String> {
        let mut stream = UnixStream::connect(self.socket_path(".socket.sock")).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        stream.write_all(payload.as_bytes()).ok()?;
        let mut out = String::new();
        stream.read_to_string(&mut out).ok()?;
        Some(out)
    }
}

impl WindowTracker for HyprlandTracker {
    fn snapshot(&self) -> Vec<WindowInfo> {
        let active_ws_id = self
            .request("j/activeworkspace")
            .and_then(|j| serde_json::from_str::<HyprWorkspaceRef>(&j).ok())
            .map(|w| w.id);
        self.request("j/clients")
            .map(|json| parse_hyprland_clients_with_workspace(&json, active_ws_id))
            .unwrap_or_default()
    }

    fn focus_window(&self, id: &str) {
        let resp = self.request(&format!("dispatch focuswindow address:{id}"));
        if !resp.as_deref().map(|r| r.trim() == "ok").unwrap_or(false) {
            let escaped = id.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = self.request(&format!(
                "dispatch hl.dsp.focus({{ window = \"address:{escaped}\" }})"
            ));
        }
    }

    fn close_window(&self, id: &str) {
        let resp = self.request(&format!("dispatch closewindow address:{id}"));
        if !resp.as_deref().map(|r| r.trim() == "ok").unwrap_or(false) {
            let escaped = id.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = self.request(&format!(
                "dispatch hl.dsp.window.close(\"address:{escaped}\")"
            ));
        }
    }

    fn minimize_window(&self, id: &str) {
        let resp = self.request(&format!(
            "dispatch movetoworkspacesilent special:minimized,address:{id}"
        ));
        if !resp.as_deref().map(|r| r.trim() == "ok").unwrap_or(false) {
            let escaped = id.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = self.request(&format!(
                "dispatch hl.dsp.window.move({{ workspace = \"special:minimized\", silent = true, window = \"address:{escaped}\" }})"
            ));
        }
    }

    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        let active_id = self
            .request("j/activeworkspace")
            .and_then(|j| serde_json::from_str::<HyprWorkspaceRef>(&j).ok())
            .map(|w| w.id);
        let Some(raw) = self.request("j/workspaces") else {
            return Vec::new();
        };
        let Ok(mut list) = serde_json::from_str::<Vec<HyprWorkspaceRef>>(&raw) else {
            return Vec::new();
        };
        list.retain(|w| w.id > 0);
        list.sort_by_key(|w| w.id);
        list.into_iter()
            .map(|w| {
                let label = if w.name.is_empty() {
                    w.id.to_string()
                } else {
                    w.name
                };
                WorkspaceInfo {
                    id: w.id.to_string(),
                    name: label,
                    active: Some(w.id) == active_id,
                }
            })
            .collect()
    }

    fn switch_workspace(&self, id: &str) {
        let resp = self.request(&format!("dispatch workspace {id}"));
        if !resp.as_deref().map(|r| r.trim() == "ok").unwrap_or(false) {
            if let Ok(num) = id.parse::<i32>() {
                let _ = self.request(&format!(
                    "dispatch hl.dsp.focus({{ workspace = {num} }})"
                ));
            }
        }
    }

    fn show_desktop(&self) {
        let resp = self.request("dispatch workspace empty");
        if !resp.as_deref().map(|r| r.trim() == "ok").unwrap_or(false) {
            let _ = self.request("dispatch hl.dsp.focus({ workspace = \"empty\" })");
        }
    }

    fn start_listener(&self, tx: async_channel::Sender<Vec<WindowInfo>>) {
        let sig = self.signature.clone();
        std::thread::spawn(move || {
            let tracker = HyprlandTracker { signature: sig };
            let _ = tx.send_blocking(tracker.snapshot());
            let mut backoff_ms = 250_u64;

            loop {
                let sock2 = tracker.socket_path(".socket2.sock");
                match UnixStream::connect(&sock2) {
                    Ok(stream) => {
                        backoff_ms = 250;
                        let reader = BufReader::new(stream);
                        for line in reader.lines().map_while(Result::ok) {
                            let relevant = line.starts_with("openwindow>>")
                                || line.starts_with("closewindow>>")
                                || line.starts_with("activewindow>>")
                                || line.starts_with("movewindow>>")
                                || line.starts_with("changefloatingmode>>")
                                || line.starts_with("fullscreen>>")
                                || line.starts_with("workspace>>")
                                || line.starts_with("workspacev2>>")
                                || line.starts_with("focusedmon>>")
                                || line.starts_with("windowtitle>>");
                            if relevant && tx.send_blocking(tracker.snapshot()).is_err() {
                                return;
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
    fn hyprland_clients_json_parses_windows() {
        let json = r#"[{"address":"0x123","class":"firefox","title":"Arch","at":[100,100],"size":[800,600],"focusHistoryID":0,"mapped":true,"hidden":false}]"#;
        let wins = parse_hyprland_clients(json);
        assert_eq!(wins.len(), 1);
        assert_eq!(wins[0].app_id, "firefox");
        assert!(wins[0].focused);
        assert_eq!(
            wins[0].geometry,
            Some(Rect {
                x: 100,
                y: 100,
                w: 800,
                h: 600
            })
        );
    }

    #[test]
    fn hyprland_malformed_json_returns_empty() {
        assert!(parse_hyprland_clients("not json").is_empty());
    }
}
