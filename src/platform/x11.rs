use crate::platform::tracker::{Rect, WindowInfo, WindowTracker, WorkspaceInfo};
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt, EventMask,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub struct X11Tracker;

impl X11Tracker {
    pub fn is_available() -> bool {
        std::env::var("DISPLAY")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some()
            && RustConnection::connect(None).is_ok()
    }

    fn query_windows(conn: &RustConnection, root: u32) -> Vec<WindowInfo> {
        let Ok(net_client_list) = intern(conn, b"_NET_CLIENT_LIST") else {
            return Vec::new();
        };
        let net_active = intern(conn, b"_NET_ACTIVE_WINDOW").ok();
        let net_wm_name = intern(conn, b"_NET_WM_NAME").ok();
        let utf8_string = intern(conn, b"UTF8_STRING").ok();
        let net_current_desktop = intern(conn, b"_NET_CURRENT_DESKTOP").ok();
        let net_wm_desktop = intern(conn, b"_NET_WM_DESKTOP").ok();
        let net_wm_state = intern(conn, b"_NET_WM_STATE").ok();
        let state_hidden = intern(conn, b"_NET_WM_STATE_HIDDEN").ok();
        let state_skip_taskbar = intern(conn, b"_NET_WM_STATE_SKIP_TASKBAR").ok();
        let net_wm_type = intern(conn, b"_NET_WM_WINDOW_TYPE").ok();
        let type_desktop = intern(conn, b"_NET_WM_WINDOW_TYPE_DESKTOP").ok();
        let type_dock = intern(conn, b"_NET_WM_WINDOW_TYPE_DOCK").ok();
        let type_splash = intern(conn, b"_NET_WM_WINDOW_TYPE_SPLASH").ok();

        let current_desktop = net_current_desktop
            .and_then(|atom| conn.get_property(false, root, atom, AtomEnum::CARDINAL, 0, 1).ok())
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next());

        let active_win = net_active
            .and_then(|atom| conn.get_property(false, root, atom, AtomEnum::WINDOW, 0, 1).ok())
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next());

        let Ok(cookie) = conn.get_property(false, root, net_client_list, AtomEnum::WINDOW, 0, 1024)
        else {
            return Vec::new();
        };
        let Ok(reply) = cookie.reply() else {
            return Vec::new();
        };
        let Some(win_ids) = reply.value32() else {
            return Vec::new();
        };

        let mut windows = Vec::new();
        for wid in win_ids {
            let wm_class = conn
                .get_property(false, wid, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| {
                    let parts: Vec<&[u8]> = r.value.split(|&b| b == 0).filter(|s| !s.is_empty()).collect();
                    let chosen = parts.last().copied().unwrap_or(&[]);
                    String::from_utf8_lossy(chosen).into_owned()
                })
                .unwrap_or_default();

            if wm_class.is_empty()
                || wm_class.eq_ignore_ascii_case("velodock")
                || wm_class.eq_ignore_ascii_case("plasmashell")
                || wm_class.eq_ignore_ascii_case("krunner")
                || wm_class.eq_ignore_ascii_case("xfce4-panel")
                || wm_class.eq_ignore_ascii_case("xfdesktop")
                || wm_class.eq_ignore_ascii_case("mate-panel")
                || wm_class.eq_ignore_ascii_case("lxqt-panel")
                || wm_class.eq_ignore_ascii_case("cinnamon")
                || wm_class.eq_ignore_ascii_case("plank")
                || wm_class.eq_ignore_ascii_case("latte-dock")
            {
                continue;
            }

            let is_desktop_or_dock = net_wm_type
                .and_then(|a| conn.get_property(false, wid, a, AtomEnum::ATOM, 0, 8).ok())
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(|mut it| {
                    it.any(|v| {
                        Some(v) == type_desktop || Some(v) == type_dock || Some(v) == type_splash
                    })
                }))
                .unwrap_or(false);
            if is_desktop_or_dock {
                continue;
            }

            let wm_states: Vec<u32> = net_wm_state
                .and_then(|a| conn.get_property(false, wid, a, AtomEnum::ATOM, 0, 16).ok())
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(|it| it.collect()))
                .unwrap_or_default();
            if state_skip_taskbar.map(|s| wm_states.contains(&s)).unwrap_or(false) {
                continue;
            }
            let is_minimized = state_hidden.map(|h| wm_states.contains(&h)).unwrap_or(false);

            let win_desktop = net_wm_desktop
                .and_then(|a| conn.get_property(false, wid, a, AtomEnum::CARDINAL, 0, 1).ok())
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32()?.next());
            let on_current_desktop = match (current_desktop, win_desktop) {
                (Some(cur), Some(wd)) => wd == cur || wd == u32::MAX,
                _ => true,
            };
            let on_active_workspace = on_current_desktop && !is_minimized;

            let title = net_wm_name
                .zip(utf8_string)
                .and_then(|(a, t)| conn.get_property(false, wid, a, t, 0, 256).ok())
                .and_then(|c| c.reply().ok())
                .map(|r| String::from_utf8_lossy(&r.value).into_owned())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    conn.get_property(false, wid, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|r| String::from_utf8_lossy(&r.value).into_owned())
                })
                .unwrap_or_default();

            let geometry = conn
                .get_geometry(wid)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|g| {
                    let (x, y) = conn
                        .translate_coordinates(wid, root, 0, 0)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|t| (t.dst_x as i32, t.dst_y as i32))
                        .unwrap_or((g.x as i32, g.y as i32));
                    Rect {
                        x,
                        y,
                        w: g.width as i32,
                        h: g.height as i32,
                    }
                });

            windows.push(WindowInfo {
                id: wid.to_string(),
                app_id: wm_class,
                title,
                focused: active_win == Some(wid),
                on_active_workspace,
                geometry,
            });
        }

        windows
    }

    fn send_ewmh_message(window_id: &str, atom_name: &[u8]) {
        let Ok(wid) = window_id.parse::<u32>() else {
            return;
        };
        let Ok((conn, screen_num)) = RustConnection::connect(None) else {
            return;
        };
        let root = conn.setup().roots[screen_num].root;
        let Ok(atom) = intern(&conn, atom_name) else {
            return;
        };
        let ev = ClientMessageEvent::new(32, wid, atom, [2u32, 0, 0, 0, 0]);
        let _ = conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
        let _ = conn.flush();
    }
}

fn intern(conn: &RustConnection, name: &[u8]) -> Result<u32, x11rb::errors::ConnectionError> {
    let cookie = conn.intern_atom(false, name)?;
    Ok(cookie.reply().map(|r| r.atom).unwrap_or(0))
}

impl WindowTracker for X11Tracker {
    fn snapshot(&self) -> Vec<WindowInfo> {
        let Ok((conn, screen_num)) = RustConnection::connect(None) else {
            return Vec::new();
        };
        let root = conn.setup().roots[screen_num].root;
        Self::query_windows(&conn, root)
    }

    fn focus_window(&self, id: &str) {
        let Ok(wid) = id.parse::<u32>() else {
            return;
        };
        if let Ok((conn, screen_num)) = RustConnection::connect(None) {
            let root = conn.setup().roots[screen_num].root;
            if let (Ok(net_wm_desktop), Ok(net_current_desktop)) = (
                intern(&conn, b"_NET_WM_DESKTOP"),
                intern(&conn, b"_NET_CURRENT_DESKTOP"),
            ) {
                let win_desk = conn
                    .get_property(false, wid, net_wm_desktop, AtomEnum::CARDINAL, 0, 1)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .and_then(|r| r.value32()?.next());
                let cur_desk = conn
                    .get_property(false, root, net_current_desktop, AtomEnum::CARDINAL, 0, 1)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .and_then(|r| r.value32()?.next());
                if let (Some(wd), Some(cd)) = (win_desk, cur_desk) {
                    if wd != cd && wd != u32::MAX {
                        let ev = ClientMessageEvent::new(32, root, net_current_desktop, [wd, 0, 0, 0, 0]);
                        let _ = conn.send_event(
                            false,
                            root,
                            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                            ev,
                        );
                        let _ = conn.flush();
                    }
                }
            }
        }
        Self::send_ewmh_message(id, b"_NET_ACTIVE_WINDOW");
    }

    fn close_window(&self, id: &str) {
        Self::send_ewmh_message(id, b"_NET_CLOSE_WINDOW");
    }

    fn minimize_window(&self, id: &str) {
        let Ok(wid) = id.parse::<u32>() else {
            return;
        };
        let Ok((conn, screen_num)) = RustConnection::connect(None) else {
            return;
        };
        let root = conn.setup().roots[screen_num].root;
        let Ok(atom) = intern(&conn, b"WM_CHANGE_STATE") else {
            return;
        };
        let ev = ClientMessageEvent::new(32, wid, atom, [3u32, 0, 0, 0, 0]);
        let _ = conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
        let _ = conn.flush();
    }

    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        let Ok((conn, screen_num)) = RustConnection::connect(None) else {
            return Vec::new();
        };
        let root = conn.setup().roots[screen_num].root;
        let num = intern(&conn, b"_NET_NUMBER_OF_DESKTOPS")
            .ok()
            .and_then(|a| conn.get_property(false, root, a, AtomEnum::CARDINAL, 0, 1).ok())
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next())
            .unwrap_or(0)
            .min(12);
        let cur = intern(&conn, b"_NET_CURRENT_DESKTOP")
            .ok()
            .and_then(|a| conn.get_property(false, root, a, AtomEnum::CARDINAL, 0, 1).ok())
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next())
            .unwrap_or(0);
        (0..num)
            .map(|idx| WorkspaceInfo {
                id: idx.to_string(),
                name: (idx + 1).to_string(),
                active: idx == cur,
            })
            .collect()
    }

    fn switch_workspace(&self, id: &str) {
        let Ok(idx) = id.parse::<u32>() else {
            return;
        };
        let Ok((conn, screen_num)) = RustConnection::connect(None) else {
            return;
        };
        let root = conn.setup().roots[screen_num].root;
        let Ok(atom) = intern(&conn, b"_NET_CURRENT_DESKTOP") else {
            return;
        };
        let ev = ClientMessageEvent::new(32, root, atom, [idx, 0, 0, 0, 0]);
        let _ = conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
        let _ = conn.flush();
    }

    fn show_desktop(&self) {
        let Ok((conn, screen_num)) = RustConnection::connect(None) else {
            return;
        };
        let root = conn.setup().roots[screen_num].root;
        let Ok(atom) = intern(&conn, b"_NET_SHOWING_DESKTOP") else {
            return;
        };
        let cur = conn
            .get_property(false, root, atom, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next())
            .unwrap_or(0);
        let next = if cur == 0 { 1 } else { 0 };
        let ev = ClientMessageEvent::new(32, root, atom, [next, 0, 0, 0, 0]);
        let _ = conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
        let _ = conn.flush();
    }

    fn start_listener(&self, tx: async_channel::Sender<Vec<WindowInfo>>) {
        std::thread::spawn(move || {
            let Ok((conn, screen_num)) = RustConnection::connect(None) else {
                return;
            };
            let root = conn.setup().roots[screen_num].root;
            let _ = conn.change_window_attributes(
                root,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            );
            let _ = conn.flush();
            let _ = tx.send_blocking(Self::query_windows(&conn, root));

            loop {
                match conn.wait_for_event() {
                    Ok(Event::PropertyNotify(_)) => {
                        if tx.send_blocking(Self::query_windows(&conn, root)).is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(500));
                        return;
                    }
                }
            }
        });
    }
}
