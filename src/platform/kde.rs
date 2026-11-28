use crate::platform::tracker::{Rect, WindowInfo, WindowTracker, WorkspaceInfo};
use crate::platform::x11::X11Tracker;
use gtk4::gio;
use gtk4::glib;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BRIDGE_OBJECT_PATH: &str = "/KWinBridge";
const BRIDGE_INTERFACE: &str = "io.github.velodock.KWinBridge";
const LISTENER_PLUGIN_NAME: &str = "velodock-kwin-listener";
const ACTION_PLUGIN_PREFIX: &str = "velodock-kwin-action";

const BRIDGE_INTROSPECTION_XML: &str = r#"
<node>
  <interface name="io.github.velodock.KWinBridge">
    <method name="UpdateWindows">
      <arg type="s" name="payload" direction="in"/>
    </method>
  </interface>
</node>
"#;

static ACTION_SEQ: AtomicU64 = AtomicU64::new(1);

pub struct KdeWaylandTracker {
    latest_windows: Arc<Mutex<Vec<WindowInfo>>>,
}

impl KdeWaylandTracker {
    pub fn from_env() -> Option<Self> {
        let is_wayland = std::env::var("WAYLAND_DISPLAY")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some();
        let is_kde = std::env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .map(|d| d.to_ascii_uppercase().contains("KDE"))
            .unwrap_or(false);
        if is_wayland && is_kde {
            Some(Self {
                latest_windows: Arc::new(Mutex::new(Vec::new())),
            })
        } else {
            None
        }
    }

    fn script_path(name: &str) -> PathBuf {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join(format!("velodock-{name}.js"))
    }

    fn kwin_unload_script(conn: &gio::DBusConnection, plugin_name: &str) {
        let params = glib::Variant::tuple_from_iter([glib::Variant::from(plugin_name)]);
        let _ = conn.call_sync(
            Some("org.kde.KWin"),
            "/Scripting",
            "org.kde.kwin.Scripting",
            "unloadScript",
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            500,
            gio::Cancellable::NONE,
        );
    }

    fn kwin_is_script_loaded(conn: &gio::DBusConnection, plugin_name: &str) -> bool {
        let params = glib::Variant::tuple_from_iter([glib::Variant::from(plugin_name)]);
        conn.call_sync(
            Some("org.kde.KWin"),
            "/Scripting",
            "org.kde.kwin.Scripting",
            "isScriptLoaded",
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            500,
            gio::Cancellable::NONE,
        )
        .ok()
        .and_then(|reply| reply.child_value(0).get::<bool>())
        .unwrap_or(false)
    }

    fn kwin_load_and_run_script(
        conn: &gio::DBusConnection,
        plugin_name: &str,
        script_body: &str,
    ) -> Result<PathBuf, String> {
        let path = Self::script_path(plugin_name);
        std::fs::write(&path, script_body).map_err(|e| format!("write script: {e}"))?;

        Self::kwin_unload_script(conn, plugin_name);

        let path_str = path.to_string_lossy().to_string();
        let params = glib::Variant::tuple_from_iter([
            glib::Variant::from(path_str.as_str()),
            glib::Variant::from(plugin_name),
        ]);
        let reply = conn
            .call_sync(
                Some("org.kde.KWin"),
                "/Scripting",
                "org.kde.kwin.Scripting",
                "loadScript",
                Some(&params),
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            )
            .map_err(|e| format!("loadScript: {e}"))?;

        let script_id = reply
            .child_value(0)
            .get::<i32>()
            .ok_or_else(|| "loadScript returned invalid id".to_string())?;
        if script_id < 0 {
            return Err(format!("loadScript rejected script (id={script_id})"));
        }

        let kwin6_path = format!("/Scripting/Script{script_id}");
        let run_kwin6 = conn.call_sync(
            Some("org.kde.KWin"),
            &kwin6_path,
            "org.kde.kwin.Script",
            "run",
            None,
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        );
        if run_kwin6.is_err() {
            let kwin5_path = format!("/{script_id}");
            conn.call_sync(
                Some("org.kde.KWin"),
                &kwin5_path,
                "org.kde.kwin.Script",
                "run",
                None,
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            )
            .map_err(|e| format!("script run: {e}"))?;
        }

        Ok(path)
    }

    fn run_oneshot_action(&self, script_body: &str) -> bool {
        let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
            return false;
        };
        let seq = ACTION_SEQ.fetch_add(1, Ordering::Relaxed);
        let plugin_name = format!("{ACTION_PLUGIN_PREFIX}-{}-{seq}", std::process::id());
        let res = Self::kwin_load_and_run_script(&conn, &plugin_name, script_body);
        Self::kwin_unload_script(&conn, &plugin_name);
        if let Ok(path) = res {
            let _ = std::fs::remove_file(path);
            true
        } else {
            false
        }
    }

    fn build_listener_script(service_name: &str) -> String {
        let service_js = serde_json::to_string(service_name).unwrap_or_else(|_| "\"\"".into());
        let path_js = serde_json::to_string(BRIDGE_OBJECT_PATH).unwrap_or_else(|_| "\"/KWinBridge\"".into());
        let iface_js = serde_json::to_string(BRIDGE_INTERFACE).unwrap_or_else(|_| "\"io.github.velodock.KWinBridge\"".into());

        format!(
            r#"(function() {{
    var SERVICE = {service_js};
    var OBJ_PATH = {path_js};
    var IFACE = {iface_js};
    var lastGeoPush = 0;

    function getWindows() {{
        if (typeof workspace.windowList === "function") {{
            return workspace.windowList();
        }}
        if (typeof workspace.clientList === "function") {{
            return workspace.clientList();
        }}
        return [];
    }}

    function getActiveWindow() {{
        if ("activeWindow" in workspace) {{
            return workspace.activeWindow;
        }}
        return workspace.activeClient;
    }}

    function isOnCurrentDesktop(w) {{
        if (w.onAllDesktops) {{
            return true;
        }}
        var cur = workspace.currentDesktop;
        if (w.desktops && w.desktops.length !== undefined) {{
            for (var i = 0; i < w.desktops.length; i++) {{
                if (w.desktops[i] === cur) {{
                    return true;
                }}
            }}
            return w.desktops.length === 0;
        }}
        if (typeof w.desktop === "number") {{
            return w.desktop === -1 || w.desktop === cur;
        }}
        return true;
    }}

    function collectSnapshot() {{
        var list = getWindows();
        var active = getActiveWindow();
        var out = [];
        for (var i = 0; i < list.length; i++) {{
            var w = list[i];
            if (!w) continue;
            if (w.desktopWindow || w.dock || w.splash || w.popupWindow || w.tooltip ||
                w.notification || w.criticalNotification || w.onScreenDisplay ||
                w.appletPopup || w.skipTaskbar || w.specialWindow) {{
                continue;
            }}
            var g = w.frameGeometry || w.geometry;
            var geom = null;
            if (g && g.width > 0 && g.height > 0) {{
                geom = {{
                    x: Math.round(g.x),
                    y: Math.round(g.y),
                    w: Math.round(g.width),
                    h: Math.round(g.height)
                }};
            }}
            out.push({{
                id: String(w.internalId),
                desktop_file_name: String(w.desktopFileName || ""),
                resource_class: String(w.resourceClass || ""),
                resource_name: String(w.resourceName || ""),
                title: String(w.caption || ""),
                focused: Boolean(active && w === active),
                minimized: Boolean(w.minimized),
                on_current_desktop: isOnCurrentDesktop(w),
                geometry: geom
            }});
        }}
        return out;
    }}

    function pushNow() {{
        try {{
            callDBus(SERVICE, OBJ_PATH, IFACE, "UpdateWindows", JSON.stringify(collectSnapshot()));
        }} catch (e) {{}}
    }}

    function pushGeometryThrottled() {{
        var now = Date.now();
        if (now - lastGeoPush >= 150) {{
            lastGeoPush = now;
            pushNow();
        }}
    }}

    function watchWindow(w) {{
        if (!w) return;
        if (w.captionChanged && w.captionChanged.connect) w.captionChanged.connect(pushNow);
        if (w.minimizedChanged && w.minimizedChanged.connect) w.minimizedChanged.connect(pushNow);
        if (w.desktopsChanged && w.desktopsChanged.connect) w.desktopsChanged.connect(pushNow);
        if (w.desktopChanged && w.desktopChanged.connect) w.desktopChanged.connect(pushNow);
        if (w.desktopFileNameChanged && w.desktopFileNameChanged.connect) w.desktopFileNameChanged.connect(pushNow);
        if (w.skipTaskbarChanged && w.skipTaskbarChanged.connect) w.skipTaskbarChanged.connect(pushNow);
        if (w.frameGeometryChanged && w.frameGeometryChanged.connect) w.frameGeometryChanged.connect(pushGeometryThrottled);
        if (w.interactiveMoveResizeFinished && w.interactiveMoveResizeFinished.connect) w.interactiveMoveResizeFinished.connect(pushNow);
    }}

    var initial = getWindows();
    for (var i = 0; i < initial.length; i++) {{
        watchWindow(initial[i]);
    }}

    if (workspace.windowAdded && workspace.windowAdded.connect) {{
        workspace.windowAdded.connect(function(w) {{
            watchWindow(w);
            pushNow();
        }});
    }} else if (workspace.clientAdded && workspace.clientAdded.connect) {{
        workspace.clientAdded.connect(function(w) {{
            watchWindow(w);
            pushNow();
        }});
    }}

    if (workspace.windowRemoved && workspace.windowRemoved.connect) {{
        workspace.windowRemoved.connect(pushNow);
    }} else if (workspace.clientRemoved && workspace.clientRemoved.connect) {{
        workspace.clientRemoved.connect(pushNow);
    }}

    if (workspace.windowActivated && workspace.windowActivated.connect) {{
        workspace.windowActivated.connect(pushNow);
    }} else if (workspace.clientActivated && workspace.clientActivated.connect) {{
        workspace.clientActivated.connect(pushNow);
    }}

    if (workspace.currentDesktopChanged && workspace.currentDesktopChanged.connect) {{
        workspace.currentDesktopChanged.connect(pushNow);
    }}

    pushNow();
}})();
"#
        )
    }

    fn build_focus_script(id: &str) -> String {
        let id_js = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".into());
        format!(
            r#"(function() {{
    var TARGET_ID = {id_js};
    var list = (typeof workspace.windowList === "function") ? workspace.windowList() : workspace.clientList();
    for (var i = 0; i < list.length; i++) {{
        var w = list[i];
        if (!w || String(w.internalId) !== TARGET_ID) continue;
        if (w.minimized) {{
            w.minimized = false;
        }}
        if (!w.onAllDesktops) {{
            if (w.desktops && w.desktops.length > 0) {{
                var onCur = false;
                for (var d = 0; d < w.desktops.length; d++) {{
                    if (w.desktops[d] === workspace.currentDesktop) {{
                        onCur = true;
                        break;
                    }}
                }}
                if (!onCur) {{
                    workspace.currentDesktop = w.desktops[0];
                }}
            }} else if (typeof w.desktop === "number" && w.desktop > 0 && w.desktop !== workspace.currentDesktop) {{
                workspace.currentDesktop = w.desktop;
            }}
        }}
        if ("activeWindow" in workspace) {{
            workspace.activeWindow = w;
        }} else {{
            workspace.activeClient = w;
        }}
        break;
    }}
}})();
"#
        )
    }

    fn build_close_script(id: &str) -> String {
        let id_js = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".into());
        format!(
            r#"(function() {{
    var TARGET_ID = {id_js};
    var list = (typeof workspace.windowList === "function") ? workspace.windowList() : workspace.clientList();
    for (var i = 0; i < list.length; i++) {{
        var w = list[i];
        if (!w || String(w.internalId) !== TARGET_ID) continue;
        w.closeWindow();
        break;
    }}
}})();
"#
        )
    }

    fn build_minimize_script(id: &str) -> String {
        let id_js = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".into());
        format!(
            r#"(function() {{
    var TARGET_ID = {id_js};
    var list = (typeof workspace.windowList === "function") ? workspace.windowList() : workspace.clientList();
    for (var i = 0; i < list.length; i++) {{
        var w = list[i];
        if (!w || String(w.internalId) !== TARGET_ID) continue;
        w.minimized = true;
        break;
    }}
}})();
"#
        )
    }

    fn kwin_current_desktop(conn: &gio::DBusConnection) -> Option<i32> {
        let reply = conn
            .call_sync(
                Some("org.kde.KWin"),
                "/KWin",
                "org.kde.KWin",
                "currentDesktop",
                None,
                None,
                gio::DBusCallFlags::NONE,
                200,
                gio::Cancellable::NONE,
            )
            .ok()?;
        reply.child_value(0).get::<i32>()
    }

    fn kwin_desktop_count(conn: &gio::DBusConnection) -> Option<u32> {
        let params = glib::Variant::tuple_from_iter([
            glib::Variant::from("org.kde.KWin.VirtualDesktopManager"),
            glib::Variant::from("count"),
        ]);
        let reply = conn
            .call_sync(
                Some("org.kde.KWin"),
                "/VirtualDesktopManager",
                "org.freedesktop.DBus.Properties",
                "Get",
                Some(&params),
                None,
                gio::DBusCallFlags::NONE,
                200,
                gio::Cancellable::NONE,
            )
            .ok()?;
        reply
            .child_value(0)
            .as_variant()
            .and_then(|v| v.get::<u32>())
    }
}

#[derive(Debug, Deserialize)]
struct KWinRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Debug, Deserialize)]
struct KWinRawWindow {
    id: String,
    #[serde(default)]
    desktop_file_name: String,
    #[serde(default)]
    resource_class: String,
    #[serde(default)]
    resource_name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    minimized: bool,
    #[serde(default = "default_true")]
    on_current_desktop: bool,
    #[serde(default)]
    geometry: Option<KWinRect>,
}

fn default_true() -> bool {
    true
}

fn is_ignored_app_id(app_id: &str) -> bool {
    let lower = app_id.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "velodock"
            | "io.github.velodock.app"
            | "plasmashell"
            | "org.kde.plasmashell"
            | "krunner"
            | "org.kde.krunner"
            | "ksmserver"
            | "latte-dock"
            | "plank"
    )
}

pub fn parse_kwin_windows_json(raw: &str) -> Vec<WindowInfo> {
    let Ok(items) = serde_json::from_str::<Vec<KWinRawWindow>>(raw) else {
        return Vec::new();
    };
    let mut windows = Vec::with_capacity(items.len());
    for item in items {
        let app_id = [
            item.desktop_file_name.as_str(),
            item.resource_class.as_str(),
            item.resource_name.as_str(),
        ]
        .into_iter()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();

        if item.id.is_empty()
            || app_id.is_empty()
            || is_ignored_app_id(&app_id)
            || is_ignored_app_id(&item.resource_class)
        {
            continue;
        }

        windows.push(WindowInfo {
            id: item.id,
            app_id,
            title: item.title,
            focused: item.focused,
            on_active_workspace: item.on_current_desktop && !item.minimized,
            geometry: item.geometry.map(|g| Rect {
                x: g.x,
                y: g.y,
                w: g.w,
                h: g.h,
            }),
        });
    }
    windows
}

impl WindowTracker for KdeWaylandTracker {
    fn snapshot(&self) -> Vec<WindowInfo> {
        if let Ok(guard) = self.latest_windows.lock() {
            if !guard.is_empty() {
                return guard.clone();
            }
        }
        Vec::new()
    }

    fn focus_window(&self, id: &str) {
        let script = Self::build_focus_script(id);
        if !self.run_oneshot_action(&script) && id.parse::<u32>().is_ok() && X11Tracker::is_available() {
            X11Tracker.focus_window(id);
        }
    }

    fn close_window(&self, id: &str) {
        let script = Self::build_close_script(id);
        if !self.run_oneshot_action(&script) && id.parse::<u32>().is_ok() && X11Tracker::is_available() {
            X11Tracker.close_window(id);
        }
    }

    fn minimize_window(&self, id: &str) {
        let script = Self::build_minimize_script(id);
        if !self.run_oneshot_action(&script) && id.parse::<u32>().is_ok() && X11Tracker::is_available() {
            X11Tracker.minimize_window(id);
        }
    }

    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
            return Vec::new();
        };
        let count = Self::kwin_desktop_count(&conn).unwrap_or(0).min(12);
        if count == 0 {
            return Vec::new();
        }
        let current = Self::kwin_current_desktop(&conn).unwrap_or(1);
        (1..=count as i32)
            .map(|idx| WorkspaceInfo {
                id: idx.to_string(),
                name: idx.to_string(),
                active: idx == current,
            })
            .collect()
    }

    fn switch_workspace(&self, id: &str) {
        let Ok(num) = id.parse::<i32>() else {
            return;
        };
        let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
            return;
        };
        let params = glib::Variant::tuple_from_iter([glib::Variant::from(num)]);
        let _ = conn.call_sync(
            Some("org.kde.KWin"),
            "/KWin",
            "org.kde.KWin",
            "setCurrentDesktop",
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            200,
            gio::Cancellable::NONE,
        );
    }

    fn start_listener(&self, tx: async_channel::Sender<Vec<WindowInfo>>) {
        let latest = self.latest_windows.clone();
        std::thread::spawn(move || {
            let ctx = glib::MainContext::new();
            let _ = ctx.with_thread_default(|| {
                let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
                    if X11Tracker::is_available() {
                        X11Tracker.start_listener(tx);
                    }
                    return;
                };

                let Some(unique_name) = conn.unique_name() else {
                    if X11Tracker::is_available() {
                        X11Tracker.start_listener(tx);
                    }
                    return;
                };
                let service_name = unique_name.to_string();

                let Ok(node_info) = gio::DBusNodeInfo::for_xml(BRIDGE_INTROSPECTION_XML) else {
                    return;
                };
                let Some(iface_info) = node_info.lookup_interface(BRIDGE_INTERFACE) else {
                    return;
                };

                let main_loop = glib::MainLoop::new(Some(&ctx), false);
                let tx_cb = tx.clone();
                let latest_cb = latest.clone();
                let loop_cb = main_loop.clone();

                let Ok(_reg_id) = conn
                    .register_object(BRIDGE_OBJECT_PATH, &iface_info)
                    .method_call(
                        move |_conn, _sender, _obj_path, _iface, method, params, invocation| {
                            if method == "UpdateWindows" {
                                if let Some(payload) = params.child_value(0).get::<String>() {
                                    let windows = parse_kwin_windows_json(&payload);
                                    if let Ok(mut guard) = latest_cb.lock() {
                                        *guard = windows.clone();
                                    }
                                    if tx_cb.try_send(windows).is_err() && tx_cb.is_closed() {
                                        loop_cb.quit();
                                    }
                                }
                                invocation.return_value(None);
                            } else {
                                invocation.return_error(
                                    gio::IOErrorEnum::NotSupported,
                                    &format!("Unknown method {method}"),
                                );
                            }
                        },
                    )
                    .build()
                else {
                    if X11Tracker::is_available() {
                        X11Tracker.start_listener(tx);
                    }
                    return;
                };

                let listener_script = Self::build_listener_script(&service_name);
                let script_path = match Self::kwin_load_and_run_script(
                    &conn,
                    LISTENER_PLUGIN_NAME,
                    &listener_script,
                ) {
                    Ok(p) => Some(p),
                    Err(_) => {
                        if X11Tracker::is_available() {
                            X11Tracker.start_listener(tx);
                        }
                        return;
                    }
                };

                let conn_watch = conn.clone();
                let tx_watch = tx.clone();
                let loop_watch = main_loop.clone();
                let script_watch = listener_script.clone();
                let source = glib::timeout_source_new(
                    Duration::from_secs(3),
                    None,
                    glib::Priority::DEFAULT,
                    move || {
                        if tx_watch.is_closed() {
                            loop_watch.quit();
                            return glib::ControlFlow::Break;
                        }
                        if !Self::kwin_is_script_loaded(&conn_watch, LISTENER_PLUGIN_NAME) {
                            let _ = Self::kwin_load_and_run_script(
                                &conn_watch,
                                LISTENER_PLUGIN_NAME,
                                &script_watch,
                            );
                        }
                        glib::ControlFlow::Continue
                    },
                );
                source.attach(Some(&ctx));

                main_loop.run();

                Self::kwin_unload_script(&conn, LISTENER_PLUGIN_NAME);
                if let Some(p) = script_path {
                    let _ = std::fs::remove_file(p);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kwin_wayland_and_xwayland_windows_and_filters_shells() {
        let json = r#"[
            {
                "id": "{11111111-1111-1111-1111-111111111111}",
                "desktop_file_name": "org.kde.dolphin",
                "resource_class": "dolphin",
                "resource_name": "dolphin",
                "title": "Home — Dolphin",
                "focused": true,
                "minimized": false,
                "on_current_desktop": true,
                "geometry": { "x": 100, "y": 50, "w": 1200, "h": 800 }
            },
            {
                "id": "{22222222-2222-2222-2222-222222222222}",
                "desktop_file_name": "",
                "resource_class": "Google-chrome",
                "resource_name": "google-chrome",
                "title": "GitHub",
                "focused": false,
                "minimized": true,
                "on_current_desktop": true,
                "geometry": { "x": 0, "y": 0, "w": 1920, "h": 1080 }
            },
            {
                "id": "{33333333-3333-3333-3333-333333333333}",
                "desktop_file_name": "io.github.velodock.App",
                "resource_class": "velodock",
                "resource_name": "velodock",
                "title": "VeloDock",
                "focused": false,
                "minimized": false,
                "on_current_desktop": true,
                "geometry": null
            },
            {
                "id": "{44444444-4444-4444-4444-444444444444}",
                "desktop_file_name": "org.kde.plasmashell",
                "resource_class": "plasmashell",
                "resource_name": "plasmashell",
                "title": "Plasma",
                "focused": false,
                "minimized": false,
                "on_current_desktop": true,
                "geometry": null
            }
        ]"#;

        let windows = parse_kwin_windows_json(json);
        assert_eq!(windows.len(), 2);

        assert_eq!(windows[0].id, "{11111111-1111-1111-1111-111111111111}");
        assert_eq!(windows[0].app_id, "org.kde.dolphin");
        assert!(windows[0].focused);
        assert!(windows[0].on_active_workspace);
        assert_eq!(
            windows[0].geometry,
            Some(Rect {
                x: 100,
                y: 50,
                w: 1200,
                h: 800,
            })
        );

        assert_eq!(windows[1].id, "{22222222-2222-2222-2222-222222222222}");
        assert_eq!(windows[1].app_id, "Google-chrome");
        assert!(!windows[1].focused);
        assert!(!windows[1].on_active_workspace);
    }

    #[test]
    fn malformed_kwin_json_returns_empty() {
        assert!(parse_kwin_windows_json("{broken").is_empty());
    }

    #[test]
    #[ignore]
    fn kde_live_kwin_bridge_receives_windows() {
        let Some(tracker) = KdeWaylandTracker::from_env() else {
            panic!("Not running in a KDE Wayland session");
        };
        let (tx, rx) = async_channel::unbounded();
        tracker.start_listener(tx);
        let start = std::time::Instant::now();
        let mut received = None;
        while start.elapsed() < Duration::from_secs(3) {
            if let Ok(wins) = rx.try_recv() {
                received = Some(wins);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let wins = received.expect("Timed out waiting for KWin bridge UpdateWindows callback");
        println!("Received {} windows from KWin:", wins.len());
        for w in &wins {
            println!(
                "  id={} app_id={:?} focused={} active_ws={} title={:?}",
                w.id, w.app_id, w.focused, w.on_active_workspace, w.title
            );
        }
    }
}
