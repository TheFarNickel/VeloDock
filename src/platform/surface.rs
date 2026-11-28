use crate::config::{DockPosition, VisibilityMode};
use crate::platform::tracker::Rect;
use gtk4::cairo;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn list_monitor_names() -> Vec<String> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let monitors = display.monitors();
    let mut names = Vec::new();
    for i in 0..monitors.n_items() {
        if let Some(mon) = monitors.item(i).and_then(|o| o.downcast::<gdk::Monitor>().ok()) {
            let label = mon
                .connector()
                .map(|s| s.to_string())
                .or_else(|| mon.model().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("Monitor {}", i + 1));
            names.push(label);
        }
    }
    names
}

pub fn resolve_monitor(preferred: Option<&str>) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    if let Some(target) = preferred.filter(|s| !s.is_empty()) {
        for i in 0..monitors.n_items() {
            if let Some(mon) = monitors.item(i).and_then(|o| o.downcast::<gdk::Monitor>().ok()) {
                let conn = mon.connector().map(|s| s.to_string());
                let model = mon.model().map(|s| s.to_string());
                if conn.as_deref() == Some(target) || model.as_deref() == Some(target) {
                    return Some(mon);
                }
            }
        }
    }
    monitors
        .item(0)
        .and_then(|o| o.downcast::<gdk::Monitor>().ok())
}

fn is_x11_backend(win: &gtk4::ApplicationWindow) -> bool {
    let display_type = gtk4::prelude::WidgetExt::display(win).type_().name();
    display_type.contains("X11")
}

pub fn reposition_x11_dock(
    win: &gtk4::ApplicationWindow,
    pos: DockPosition,
    edge_margin: i32,
    width: i32,
    height: i32,
    monitor_name: Option<&str>,
) {
    if gtk4_layer_shell::is_supported() || !is_x11_backend(win) || !win.is_realized() {
        return;
    }
    let mon_rect = resolve_monitor(monitor_name).map(|m| {
        let g = m.geometry();
        Rect {
            x: g.x(),
            y: g.y(),
            w: g.width(),
            h: g.height(),
        }
    });
    apply_x11_dock_hints(pos, edge_margin, None, Some((width, height)), mon_rect);
}

pub fn configure_dock_window(
    win: &gtk4::ApplicationWindow,
    pos: DockPosition,
    vis: VisibilityMode,
    edge_margin: i32,
    exclusive_px: i32,
    monitor_name: Option<&str>,
) {
    win.set_decorated(false);
    win.set_resizable(false);

    if gtk4_layer_shell::is_supported() {
        if !win.is_layer_window() {
            win.init_layer_shell();
            win.set_namespace(Some("velodock"));
            win.set_layer(Layer::Top);
            win.set_keyboard_mode(KeyboardMode::None);
        }

        let mon = resolve_monitor(monitor_name);
        win.set_monitor(mon.as_ref());

        for edge in [Edge::Bottom, Edge::Top, Edge::Left, Edge::Right] {
            win.set_anchor(edge, false);
            win.set_margin(edge, 0);
        }

        match pos {
            DockPosition::Bottom => {
                win.set_anchor(Edge::Bottom, true);
                win.set_margin(Edge::Bottom, edge_margin);
            }
            DockPosition::Top => {
                win.set_anchor(Edge::Top, true);
                win.set_margin(Edge::Top, edge_margin);
            }
            DockPosition::Left => {
                win.set_anchor(Edge::Left, true);
                win.set_margin(Edge::Left, edge_margin);
            }
            DockPosition::Right => {
                win.set_anchor(Edge::Right, true);
                win.set_margin(Edge::Right, edge_margin);
            }
        }

        let zone = match vis {
            VisibilityMode::Always => exclusive_px.max(0),
            VisibilityMode::AutoHide | VisibilityMode::Intellihide => 0,
        };
        win.set_exclusive_zone(zone);
    } else if is_x11_backend(win) {
        let strut_px = match vis {
            VisibilityMode::Always => exclusive_px.max(0) as u32,
            VisibilityMode::AutoHide | VisibilityMode::Intellihide => 0,
        };
        let mon_rect = resolve_monitor(monitor_name).map(|m| {
            let g = m.geometry();
            Rect {
                x: g.x(),
                y: g.y(),
                w: g.width(),
                h: g.height(),
            }
        });
        if win.is_realized() {
            apply_x11_dock_hints(pos, edge_margin, Some(strut_px), None, mon_rect);
        } else {
            win.connect_realize(move |_| {
                apply_x11_dock_hints(pos, edge_margin, Some(strut_px), None, mon_rect);
            });
        }
    }
}

fn apply_x11_dock_hints(
    pos: DockPosition,
    edge_margin: i32,
    strut_px: Option<u32>,
    explicit_size: Option<(i32, i32)>,
    monitor_geo: Option<Rect>,
) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt, PropMode};
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

    let Ok((conn, screen_num)) = RustConnection::connect(None) else {
        return;
    };
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let sw = screen.width_in_pixels as i32;
    let sh = screen.height_in_pixels as i32;
    let my_pid = std::process::id();

    let intern = |name: &[u8]| -> Option<u32> {
        conn.intern_atom(false, name)
            .ok()?
            .reply()
            .ok()
            .map(|r| r.atom)
    };
    let Some(net_wm_pid) = intern(b"_NET_WM_PID") else {
        return;
    };
    let Some(net_wm_type) = intern(b"_NET_WM_WINDOW_TYPE") else {
        return;
    };
    let Some(type_dock) = intern(b"_NET_WM_WINDOW_TYPE_DOCK") else {
        return;
    };
    let Some(net_wm_state) = intern(b"_NET_WM_STATE") else {
        return;
    };
    let Some(net_wm_strut_partial) = intern(b"_NET_WM_STRUT_PARTIAL") else {
        return;
    };
    let net_wm_name = intern(b"_NET_WM_NAME");
    let utf8_string = intern(b"UTF8_STRING");

    let states: Vec<u32> = [
        b"_NET_WM_STATE_STICKY".as_slice(),
        b"_NET_WM_STATE_ABOVE",
        b"_NET_WM_STATE_SKIP_TASKBAR",
        b"_NET_WM_STATE_SKIP_PAGER",
    ]
    .iter()
    .filter_map(|a| intern(a))
    .collect();

    let Ok(tree) = conn.query_tree(root).ok().and_then(|c| c.reply().ok()).ok_or(()) else {
        return;
    };

    for wid in tree.children {
        let pid = conn
            .get_property(false, wid, net_wm_pid, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next());
        if pid != Some(my_pid) {
            continue;
        }
        let title = net_wm_name
            .zip(utf8_string)
            .and_then(|(a, t)| conn.get_property(false, wid, a, t, 0, 64).ok())
            .and_then(|c| c.reply().ok())
            .map(|r| String::from_utf8_lossy(&r.value).into_owned())
            .unwrap_or_default();
        if !title.is_empty() && title != "VeloDock" {
            continue;
        }

        let _ = conn.change_property32(PropMode::REPLACE, wid, net_wm_type, AtomEnum::ATOM, &[type_dock]);
        if !states.is_empty() {
            let _ = conn.change_property32(PropMode::REPLACE, wid, net_wm_state, AtomEnum::ATOM, &states);
        }

        let mon = monitor_geo.unwrap_or(Rect {
            x: 0,
            y: 0,
            w: sw,
            h: sh,
        });
        let dims = explicit_size.or_else(|| {
            conn.get_geometry(wid)
                .ok()?
                .reply()
                .ok()
                .map(|g| (g.width as i32, g.height as i32))
        });
        if let Some((gw, gh)) = dims {
            let (tx, ty) = match pos {
                DockPosition::Bottom => (mon.x + (mon.w - gw) / 2, mon.y + mon.h - gh - edge_margin),
                DockPosition::Top => (mon.x + (mon.w - gw) / 2, mon.y + edge_margin),
                DockPosition::Left => (mon.x + edge_margin, mon.y + (mon.h - gh) / 2),
                DockPosition::Right => (mon.x + mon.w - gw - edge_margin, mon.y + (mon.h - gh) / 2),
            };
            let _ = conn.configure_window(wid, &ConfigureWindowAux::new().x(tx).y(ty));
        }

        if let Some(strut_val) = strut_px {
            let mut strut = [0u32; 12];
            if strut_val > 0 {
                let mx0 = mon.x.max(0) as u32;
                let my0 = mon.y.max(0) as u32;
                let mx1 = (mon.x + mon.w - 1).max(0) as u32;
                let my1 = (mon.y + mon.h - 1).max(0) as u32;
                match pos {
                    DockPosition::Bottom => {
                        let bottom_gap = (sh - (mon.y + mon.h)).max(0) as u32;
                        strut[3] = strut_val + bottom_gap;
                        strut[10] = mx0;
                        strut[11] = mx1;
                    }
                    DockPosition::Top => {
                        strut[2] = strut_val + my0;
                        strut[8] = mx0;
                        strut[9] = mx1;
                    }
                    DockPosition::Left => {
                        strut[0] = strut_val + mx0;
                        strut[4] = my0;
                        strut[5] = my1;
                    }
                    DockPosition::Right => {
                        let right_gap = (sw - (mon.x + mon.w)).max(0) as u32;
                        strut[1] = strut_val + right_gap;
                        strut[6] = my0;
                        strut[7] = my1;
                    }
                }
            }
            let _ = conn.change_property32(
                PropMode::REPLACE,
                wid,
                net_wm_strut_partial,
                AtomEnum::CARDINAL,
                &strut,
            );
        }
    }
    let _ = conn.flush();
}

pub fn configure_overlay_window(win: &gtk4::Window) {
    win.set_decorated(false);
    if gtk4_layer_shell::is_supported() {
        if !win.is_layer_window() {
            win.init_layer_shell();
            win.set_namespace(Some("velodock-launchpad"));
            win.set_layer(Layer::Overlay);
            win.set_keyboard_mode(KeyboardMode::Exclusive);
            win.set_exclusive_zone(-1);
        }
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            win.set_anchor(edge, true);
            win.set_margin(edge, 0);
        }
    } else {
        win.fullscreen();
    }
}

pub fn update_input_region(win: &gtk4::ApplicationWindow, rect: Option<Rect>) {
    let Some(surface) = win.surface() else {
        return;
    };
    if let Some(r) = rect {
        let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(
            r.x,
            r.y,
            r.w.max(1),
            r.h.max(1),
        ));
        surface.set_input_region(Some(&region));
    } else {
        surface.set_input_region(None);
    }
}
