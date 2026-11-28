use crate::config::DockPosition;
use crate::ui::dock_window::DockItemKind;
use crate::ui::AppState;
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

fn popover_position_for_dock(pos: DockPosition) -> gtk4::PositionType {
    match pos {
        DockPosition::Bottom => gtk4::PositionType::Top,
        DockPosition::Top => gtk4::PositionType::Bottom,
        DockPosition::Left => gtk4::PositionType::Right,
        DockPosition::Right => gtk4::PositionType::Left,
    }
}

pub fn show_item_menu(
    parent: &gtk4::DrawingArea,
    x: f64,
    y: f64,
    dock_pos: DockPosition,
    item_kind: Option<DockItemKind>,
    is_pinned: bool,
    window_ids: Vec<String>,
    state: Rc<RefCell<AppState>>,
    on_refresh: Rc<dyn Fn()>,
    on_open_settings: Rc<dyn Fn()>,
) {
    let popover = gtk4::Popover::new();
    popover.set_parent(parent);
    popover.add_css_class("archdock-menu");
    popover.set_has_arrow(true);
    popover.set_position(popover_position_for_dock(dock_pos));
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    vbox.set_size_request(180, -1);

    let add_btn = |label: &str, destructive: bool, on_click: Box<dyn Fn()>| {
        let btn = gtk4::Button::with_label(label);
        btn.add_css_class("archdock-menu-item");
        if destructive {
            btn.add_css_class("destructive");
        }
        btn.set_halign(gtk4::Align::Fill);
        if let Some(child) = btn.child().and_then(|c| c.downcast::<gtk4::Label>().ok()) {
            child.set_xalign(0.0);
        }
        let pop_clone = popover.clone();
        btn.connect_clicked(move |_| {
            pop_clone.popdown();
            on_click();
        });
        vbox.append(&btn);
    };

    match item_kind {
        Some(DockItemKind::App { desktop_id, .. }) => {
            let app_title = state
                .borrow()
                .desktop_index
                .get(&desktop_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| desktop_id.trim_end_matches(".desktop").to_string());
            if !app_title.is_empty() {
                let hdr = gtk4::Label::new(Some(&app_title));
                hdr.set_halign(gtk4::Align::Start);
                hdr.add_css_class("archdock-menu-header");
                vbox.append(&hdr);
            }
            let is_running = !window_ids.is_empty();
            let state_c = state.clone();
            let id_c = desktop_id.clone();
            add_btn(
                if is_running { "New Window" } else { "Open" },
                false,
                Box::new(move || {
                    let st = state_c.borrow();
                    if let Some(entry) = st.desktop_index.get(&id_c) {
                        let _ = entry.launch();
                    }
                }),
            );

            let state_c = state.clone();
            let id_c = desktop_id.clone();
            let refresh_c = on_refresh.clone();
            if is_pinned {
                add_btn(
                    "Remove from Dock",
                    false,
                    Box::new(move || {
                        {
                            let mut st = state_c.borrow_mut();
                            st.config.apps.pinned.retain(|p| p != &id_c);
                        }
                        AppState::schedule_save(&state_c);
                        refresh_c();
                    }),
                );
            } else {
                add_btn(
                    "Keep in Dock",
                    false,
                    Box::new(move || {
                        {
                            let mut st = state_c.borrow_mut();
                            if !st.config.apps.pinned.contains(&id_c) {
                                st.config.apps.pinned.push(id_c.clone());
                            }
                        }
                        AppState::schedule_save(&state_c);
                        refresh_c();
                    }),
                );
            }

            if is_running {
                let state_c = state.clone();
                let wins_c = window_ids.clone();
                add_btn(
                    "Quit",
                    true,
                    Box::new(move || {
                        let st = state_c.borrow();
                        for wid in &wins_c {
                            st.tracker.close_window(wid);
                        }
                    }),
                );
            }

            vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        }
        Some(DockItemKind::Trash) => {
            add_btn(
                "Open Trash",
                false,
                Box::new(|| {
                    let _ = crate::trash::TrashDir::open_trash();
                }),
            );
            let is_empty = state.borrow().trash.is_empty();
            if !is_empty {
                let state_c = state.clone();
                let refresh_c = on_refresh.clone();
                let parent_win = parent.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
                add_btn(
                    "Empty Trash...",
                    true,
                    Box::new(move || {
                        let dialog = gtk4::AlertDialog::builder()
                            .message("Empty Trash?")
                            .detail("All items in the Trash will be permanently deleted.")
                            .buttons(["Cancel", "Empty Trash"])
                            .cancel_button(0)
                            .default_button(1)
                            .build();
                        let state_cc = state_c.clone();
                        let refresh_cc = refresh_c.clone();
                        dialog.choose(parent_win.as_ref(), gtk4::gio::Cancellable::NONE, move |res| {
                            if res == Ok(1) {
                                let _ = state_cc.borrow().trash.empty_trash();
                                refresh_cc();
                            }
                        });
                    }),
                );
            }
            vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        }
        Some(DockItemKind::WindowTile { window_id, title }) => {
            let hdr = gtk4::Label::new(Some(&title));
            hdr.set_halign(gtk4::Align::Start);
            hdr.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            hdr.set_max_width_chars(24);
            hdr.add_css_class("archdock-menu-header");
            vbox.append(&hdr);

            let st_f = state.clone();
            let wid_f = window_id.clone();
            add_btn(
                "Switch to Window",
                false,
                Box::new(move || {
                    st_f.borrow().tracker.focus_window(&wid_f);
                }),
            );

            let st_c = state.clone();
            let wid_c = window_id.clone();
            add_btn(
                "Close Window",
                true,
                Box::new(move || {
                    st_c.borrow().tracker.close_window(&wid_c);
                }),
            );
            vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        }
        Some(DockItemKind::Folder { path }) => {
            let folder_name = std::path::Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&path)
                .to_string();
            let hdr = gtk4::Label::new(Some(&folder_name));
            hdr.set_halign(gtk4::Align::Start);
            hdr.add_css_class("archdock-menu-header");
            vbox.append(&hdr);

            let open_path = path.clone();
            add_btn(
                "Open in File Manager",
                false,
                Box::new(move || {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(&open_path)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                }),
            );

            let state_c = state.clone();
            let rm_path = path.clone();
            let refresh_c = on_refresh.clone();
            add_btn(
                "Remove Stack from Dock",
                true,
                Box::new(move || {
                    state_c
                        .borrow_mut()
                        .config
                        .apps
                        .pinned_folders
                        .retain(|f| f != &rm_path);
                    AppState::schedule_save(&state_c);
                    refresh_c();
                }),
            );
            vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        }
        Some(DockItemKind::Launchpad) | None => {
            let hdr = gtk4::Label::new(Some("VeloDock"));
            hdr.set_halign(gtk4::Align::Start);
            hdr.add_css_class("archdock-menu-header");
            vbox.append(&hdr);
        }
    }

    let settings_c = on_open_settings.clone();
    add_btn(
        "Dock Settings...",
        false,
        Box::new(move || {
            settings_c();
        }),
    );

    popover.set_child(Some(&vbox));
    popover.connect_closed(|p| {
        let pop = p.clone();
        gtk4::glib::idle_add_local_once(move || {
            pop.unparent();
        });
    });
    popover.popup();
}

pub fn show_folder_stack_popover(
    parent: &gtk4::DrawingArea,
    x: f64,
    y: f64,
    dock_pos: DockPosition,
    folder_path: &str,
) {
    let popover = gtk4::Popover::new();
    popover.set_parent(parent);
    popover.add_css_class("archdock-menu");
    popover.set_has_arrow(true);
    popover.set_position(popover_position_for_dock(dock_pos));
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    vbox.set_size_request(260, -1);

    let title = std::path::Path::new(folder_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(folder_path);
    let hdr = gtk4::Label::new(Some(title));
    hdr.set_halign(gtk4::Align::Start);
    hdr.add_css_class("archdock-menu-header");
    vbox.append(&hdr);

    let mut entries: Vec<(std::path::PathBuf, bool, std::time::SystemTime)> =
        std::fs::read_dir(folder_path)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let name = p.file_name()?.to_str()?;
                if name.starts_with('.') {
                    return None;
                }
                let meta = e.metadata().ok()?;
                let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                Some((p, meta.is_dir(), mtime))
            })
            .collect();
    entries.sort_by(|a, b| b.2.cmp(&a.2));
    entries.truncate(14);

    if entries.is_empty() {
        let empty = gtk4::Label::new(Some("Folder is empty"));
        empty.add_css_class("settings-muted");
        empty.set_margin_top(6);
        empty.set_margin_bottom(6);
        vbox.append(&empty);
    } else {
        for (path, is_dir, _) in entries {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let btn = gtk4::Button::new();
            btn.add_css_class("archdock-menu-item");
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let icon = gtk4::Image::from_icon_name(if is_dir {
                "folder"
            } else {
                "text-x-generic"
            });
            icon.set_pixel_size(16);
            let lbl = gtk4::Label::new(Some(&name));
            lbl.set_halign(gtk4::Align::Start);
            lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lbl.set_max_width_chars(28);
            row.append(&icon);
            row.append(&lbl);
            btn.set_child(Some(&row));

            let pop_c = popover.clone();
            btn.connect_clicked(move |_| {
                pop_c.popdown();
                let _ = std::process::Command::new("xdg-open")
                    .arg(&path)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            });
            vbox.append(&btn);
        }
    }

    vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    let open_all = gtk4::Button::with_label("Open in File Manager");
    open_all.add_css_class("archdock-menu-item");
    let pop_c = popover.clone();
    let dir_c = folder_path.to_string();
    open_all.connect_clicked(move |_| {
        pop_c.popdown();
        let _ = std::process::Command::new("xdg-open")
            .arg(&dir_c)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    });
    vbox.append(&open_all);

    popover.set_child(Some(&vbox));
    popover.connect_closed(|p| {
        let pop = p.clone();
        gtk4::glib::idle_add_local_once(move || {
            pop.unparent();
        });
    });
    popover.popup();
}
