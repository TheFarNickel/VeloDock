use crate::config::parse_hex_rgb;
use crate::platform::surface::configure_overlay_window;
use crate::ui::AppState;
use crate::widgets::eval_math_expr;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const CATEGORIES: &[(&str, &str, &[&str])] = &[
    ("all", "All", &[]),
    ("internet", "Internet", &["Network", "WebBrowser", "Email", "InstantMessaging"]),
    ("dev", "Dev", &["Development", "IDE", "TextEditor"]),
    ("media", "Media", &["AudioVideo", "Audio", "Video", "Player"]),
    ("graphics", "Graphics", &["Graphics", "Photography", "2DGraphics", "3DGraphics"]),
    ("games", "Games", &["Game"]),
    ("system", "System", &["System", "Settings", "Utility", "TerminalEmulator", "FileManager"]),
];

pub struct LaunchpadWindow {
    window: gtk4::Window,
    backdrop: gtk4::Box,
    content: gtk4::Box,
    search_entry: gtk4::SearchEntry,
    quick_btn: gtk4::Button,
    category_bar: gtk4::Box,
    flow: gtk4::FlowBox,
    dots_box: gtk4::Box,
    state: Rc<RefCell<AppState>>,
    on_dock_refresh: Rc<dyn Fn()>,
    filtered_ids: Rc<RefCell<Vec<String>>>,
    selected_category: RefCell<String>,
    current_page: Cell<usize>,
    total_pages: Cell<usize>,
    is_open: Cell<bool>,
    close_timer: RefCell<Option<glib::SourceId>>,
    dynamic_css: gtk4::CssProvider,
}

impl LaunchpadWindow {
    pub fn new(
        app: &gtk4::Application,
        state: Rc<RefCell<AppState>>,
        on_dock_refresh: Rc<dyn Fn()>,
    ) -> Rc<Self> {
        let window = gtk4::Window::builder()
            .application(app)
            .title("VeloDock Launchpad")
            .build();
        window.add_css_class("archdock-launchpad");
        configure_overlay_window(&window);

        let backdrop = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        backdrop.add_css_class("launchpad-backdrop");
        backdrop.set_hexpand(true);
        backdrop.set_vexpand(true);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        content.add_css_class("launchpad-content");
        content.set_hexpand(true);
        content.set_vexpand(true);
        content.set_margin_top(42);
        content.set_margin_bottom(32);
        content.set_margin_start(64);
        content.set_margin_end(64);

        let search_entry = gtk4::SearchEntry::new();
        search_entry.add_css_class("launchpad-search");
        search_entry.set_placeholder_text(Some("Search apps, calc (25*4), > cmd, or ? web…"));
        search_entry.set_halign(gtk4::Align::Center);
        content.append(&search_entry);

        let quick_btn = gtk4::Button::new();
        quick_btn.add_css_class("launchpad-cat-pill");
        quick_btn.add_css_class("active");
        quick_btn.set_halign(gtk4::Align::Center);
        quick_btn.set_visible(false);
        content.append(&quick_btn);

        let category_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        category_bar.set_halign(gtk4::Align::Center);
        content.append(&category_bar);

        let flow = gtk4::FlowBox::new();
        flow.set_valign(gtk4::Align::Start);
        flow.set_row_spacing(18);
        flow.set_column_spacing(18);
        flow.set_homogeneous(true);
        flow.set_selection_mode(gtk4::SelectionMode::None);

        let scroller = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&flow)
            .build();
        content.append(&scroller);

        let dots_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        dots_box.set_halign(gtk4::Align::Center);
        dots_box.set_margin_top(6);
        content.append(&dots_box);

        backdrop.append(&content);
        window.set_child(Some(&backdrop));

        let dynamic_css = gtk4::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &dynamic_css,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );
        }

        let lp = Rc::new(Self {
            window,
            backdrop,
            content,
            search_entry,
            quick_btn,
            category_bar,
            flow,
            dots_box,
            state,
            on_dock_refresh,
            filtered_ids: Rc::new(RefCell::new(Vec::new())),
            selected_category: RefCell::new("all".to_string()),
            current_page: Cell::new(0),
            total_pages: Cell::new(1),
            is_open: Cell::new(false),
            close_timer: RefCell::new(None),
            dynamic_css,
        });

        lp.build_category_pills();
        lp.wire_events();
        lp.apply_config();
        lp
    }

    pub fn apply_config(self: &Rc<Self>) {
        let lp_cfg = self.state.borrow().config.launchpad.clone();
        let (r, g, b) = lp_cfg
            .custom_bg
            .as_deref()
            .and_then(parse_hex_rgb)
            .map(|(r, g, b)| ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8))
            .unwrap_or((14, 15, 22));
        let alpha = lp_cfg.backdrop_opacity.clamp(0.15, 0.96);
        let scale = lp_cfg.tile_hover_scale.clamp(1.0, 1.24);

        self.dynamic_css.load_from_string(&format!(
            ".launchpad-backdrop {{ background-color: rgba({r}, {g}, {b}, {alpha:.2}); }}\n\
             button.launchpad-tile:hover, button.launchpad-tile:focus {{ transform: scale({scale:.2}); }}\n"
        ));

        let cols = lp_cfg.columns.clamp(4, 10);
        self.flow.set_max_children_per_line(cols);
        self.flow.set_min_children_per_line(cols.min(4));
        self.category_bar.set_visible(lp_cfg.show_categories);

        let query = self.search_entry.text().to_string();
        self.populate(&query);
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.is_open.get() {
            self.close_animated();
        } else {
            self.open_animated();
        }
    }

    pub fn open_animated(self: &Rc<Self>) {
        if let Some(id) = self.close_timer.borrow_mut().take() {
            id.remove();
        }
        self.is_open.set(true);
        self.current_page.set(0);
        self.search_entry.set_text("");
        self.apply_config();

        self.backdrop.remove_css_class("open");
        self.content.remove_css_class("open");
        self.window.present();
        self.search_entry.grab_focus();

        let backdrop = self.backdrop.clone();
        let content = self.content.clone();
        glib::timeout_add_local_once(Duration::from_millis(16), move || {
            backdrop.add_css_class("open");
            content.add_css_class("open");
        });
    }

    pub fn close_animated(self: &Rc<Self>) {
        if !self.is_open.get() {
            return;
        }
        self.is_open.set(false);
        self.backdrop.remove_css_class("open");
        self.content.remove_css_class("open");

        if let Some(id) = self.close_timer.borrow_mut().take() {
            id.remove();
        }
        let lp_w = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(220), move || {
            if let Some(lp) = lp_w.upgrade() {
                *lp.close_timer.borrow_mut() = None;
                if !lp.is_open.get() {
                    lp.window.set_visible(false);
                }
            }
        });
        *self.close_timer.borrow_mut() = Some(id);
    }

    fn build_category_pills(self: &Rc<Self>) {
        let pills: Rc<RefCell<Vec<(String, gtk4::Button)>>> = Rc::new(RefCell::new(Vec::new()));
        for &(id, label, _) in CATEGORIES {
            let btn = gtk4::Button::with_label(label);
            btn.add_css_class("launchpad-cat-pill");
            if id == "all" {
                btn.add_css_class("active");
            }
            let lp_w = Rc::downgrade(self);
            let pills_c = pills.clone();
            let cat_id = id.to_string();
            btn.connect_clicked(move |_| {
                for (pid, b) in pills_c.borrow().iter() {
                    if pid == &cat_id {
                        b.add_css_class("active");
                    } else {
                        b.remove_css_class("active");
                    }
                }
                if let Some(lp) = lp_w.upgrade() {
                    *lp.selected_category.borrow_mut() = cat_id.clone();
                    lp.current_page.set(0);
                    let q = lp.search_entry.text().to_string();
                    lp.populate(&q);
                }
            });
            pills.borrow_mut().push((id.to_string(), btn.clone()));
            self.category_bar.append(&btn);
        }
    }

    fn change_page(self: &Rc<Self>, delta: isize) {
        let total = self.total_pages.get();
        if total <= 1 {
            return;
        }
        let cur = self.current_page.get() as isize;
        let next = (cur + delta).clamp(0, (total - 1) as isize) as usize;
        if next != self.current_page.get() {
            self.current_page.set(next);
            let q = self.search_entry.text().to_string();
            self.populate(&q);
        }
    }

    fn wire_events(self: &Rc<Self>) {

        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let lp_w = Rc::downgrade(self);
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            let Some(lp) = lp_w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match key {
                gdk::Key::Escape => {
                    lp.close_animated();
                    glib::Propagation::Stop
                }
                gdk::Key::Left if lp.state.borrow().config.launchpad.paged_mode && lp.search_entry.text().is_empty() => {
                    lp.change_page(-1);
                    glib::Propagation::Stop
                }
                gdk::Key::Right if lp.state.borrow().config.launchpad.paged_mode && lp.search_entry.text().is_empty() => {
                    lp.change_page(1);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.window.add_controller(key_ctrl);

        let scroll = gtk4::EventControllerScroll::new(
            gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::DISCRETE,
        );
        let lp_w = Rc::downgrade(self);
        scroll.connect_scroll(move |_, _dx, dy| {
            let Some(lp) = lp_w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !lp.state.borrow().config.launchpad.paged_mode || lp.total_pages.get() <= 1 {
                return glib::Propagation::Proceed;
            }
            if dy > 0.0 {
                lp.change_page(1);
                glib::Propagation::Stop
            } else if dy < 0.0 {
                lp.change_page(-1);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.window.add_controller(scroll);

        let lp_w = Rc::downgrade(self);
        self.search_entry.connect_stop_search(move |_| {
            if let Some(lp) = lp_w.upgrade() {
                lp.close_animated();
            }
        });

        let lp_w = Rc::downgrade(self);
        self.search_entry.connect_search_changed(move |entry| {
            if let Some(lp) = lp_w.upgrade() {
                lp.current_page.set(0);
                lp.populate(&entry.text());
            }
        });

        let lp_w = Rc::downgrade(self);
        self.quick_btn.connect_clicked(move |_| {
            if let Some(lp) = lp_w.upgrade() {
                lp.execute_quick_action();
            }
        });

        let lp_w = Rc::downgrade(self);
        self.search_entry.connect_activate(move |_| {
            if let Some(lp) = lp_w.upgrade() {
                if lp.execute_quick_action() {
                    return;
                }
                let first_id = lp.filtered_ids.borrow().first().cloned();
                if let Some(id) = first_id {
                    if let Some(entry) = lp.state.borrow().desktop_index.get(&id) {
                        let _ = entry.launch();
                    }
                    lp.close_animated();
                }
            }
        });

        let click = gtk4::GestureClick::builder().button(1).build();
        let lp_w = Rc::downgrade(self);
        let flow_c = self.flow.clone();
        click.connect_released(move |_, _, x, y| {
            if flow_c.child_at_pos(x as i32, y as i32).is_none() {
                if let Some(lp) = lp_w.upgrade() {
                    lp.close_animated();
                }
            }
        });
        self.flow.add_controller(click);
    }

    fn execute_quick_action(self: &Rc<Self>) -> bool {
        let raw = self.search_entry.text().to_string();
        let trimmed = raw.trim();
        if let Some(cmd) = trimmed.strip_prefix('>').map(str::trim).filter(|s| !s.is_empty()) {
            let term = ["kitty", "alacritty", "foot", "wezterm", "konsole", "gnome-terminal", "xterm"]
                .into_iter()
                .find(|t| {
                    std::process::Command::new("which")
                        .arg(t)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                });
            if let Some(t) = term {
                let _ = std::process::Command::new(t)
                    .args(["-e", "sh", "-c", &format!("{cmd}; exec sh")])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            } else {
                let _ = std::process::Command::new("sh")
                    .args(["-c", cmd])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            self.close_animated();
            return true;
        }
        if let Some(q) = trimmed.strip_prefix('?').map(str::trim).filter(|s| !s.is_empty()) {
            let encoded = percent_encoding::utf8_percent_encode(q, percent_encoding::NON_ALPHANUMERIC);
            let url = format!("https://duckduckgo.com/?q={encoded}");
            let _ = std::process::Command::new("xdg-open")
                .arg(url)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            self.close_animated();
            return true;
        }
        if let Some(val) = eval_math_expr(trimmed) {
            let formatted = if (val - val.round()).abs() < 1e-9 {
                format!("{}", val.round() as i64)
            } else {
                format!("{val:.6}").trim_end_matches('0').trim_end_matches('.').to_string()
            };
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text(&formatted);
            }
            self.close_animated();
            return true;
        }
        false
    }

    fn populate(self: &Rc<Self>, query: &str) {
        let trimmed = query.trim();
        if let Some(cmd) = trimmed.strip_prefix('>').map(str::trim).filter(|s| !s.is_empty()) {
            self.quick_btn.set_label(&format!("Run in Terminal:  {cmd}   ⏎"));
            self.quick_btn.set_visible(true);
        } else if let Some(q) = trimmed.strip_prefix('?').map(str::trim).filter(|s| !s.is_empty()) {
            self.quick_btn.set_label(&format!("Search Web:  {q}   ⏎"));
            self.quick_btn.set_visible(true);
        } else if let Some(val) = eval_math_expr(trimmed) {
            let formatted = if (val - val.round()).abs() < 1e-9 {
                format!("{}", val.round() as i64)
            } else {
                format!("{val:.6}").trim_end_matches('0').trim_end_matches('.').to_string()
            };
            self.quick_btn.set_label(&format!("=  {formatted}   (Enter to copy)"));
            self.quick_btn.set_visible(true);
        } else {
            self.quick_btn.set_visible(false);
        }

        while let Some(child) = self.flow.first_child() {
            self.flow.remove(&child);
        }
        while let Some(child) = self.dots_box.first_child() {
            self.dots_box.remove(&child);
        }

        let lp_cfg = self.state.borrow().config.launchpad.clone();
        let q = query.trim().to_lowercase();
        let cat = self.selected_category.borrow().clone();
        let cat_tags: &[&str] = CATEGORIES
            .iter()
            .find(|(id, _, _)| *id == cat.as_str())
            .map(|(_, _, tags)| *tags)
            .unwrap_or(&[]);

        let all_entries: Vec<(String, String, Option<String>)> = {
            let st = self.state.borrow();
            st.desktop_index
                .visible_sorted()
                .into_iter()
                .filter(|e| {
                    let match_q = q.is_empty() || e.name.to_lowercase().contains(&q);
                    let match_cat = cat_tags.is_empty()
                        || e.categories
                            .iter()
                            .any(|c| cat_tags.iter().any(|t| c.eq_ignore_ascii_case(t)));
                    match_q && match_cat
                })
                .map(|e| (e.id.clone(), e.name.clone(), e.icon.clone()))
                .collect()
        };

        let mut ids = self.filtered_ids.borrow_mut();
        ids.clear();
        for (id, _, _) in &all_entries {
            ids.push(id.clone());
        }
        drop(ids);

        let per_page = (lp_cfg.columns * lp_cfg.rows).max(1) as usize;
        let pages = if lp_cfg.paged_mode && !all_entries.is_empty() {
            all_entries.len().div_ceil(per_page)
        } else {
            1
        };
        self.total_pages.set(pages);
        let cur_page = self.current_page.get().min(pages.saturating_sub(1));
        self.current_page.set(cur_page);

        let slice = if lp_cfg.paged_mode {
            let start = cur_page * per_page;
            let end = (start + per_page).min(all_entries.len());
            &all_entries[start..end]
        } else {
            &all_entries[..]
        };

        let icon_px = lp_cfg.icon_size as i32;
        for (id, name, icon_opt) in slice {
            let tile = gtk4::Button::new();
            tile.add_css_class("launchpad-tile");

            let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
            vbox.set_halign(gtk4::Align::Center);
            vbox.set_size_request(icon_px + 48, icon_px + if lp_cfg.show_labels { 44 } else { 18 });

            let img = match icon_opt.as_deref() {
                Some(ic) if !ic.is_empty() => {
                    if std::path::Path::new(ic).is_absolute() {
                        gtk4::Image::from_file(ic)
                    } else {
                        gtk4::Image::from_icon_name(ic)
                    }
                }
                _ => gtk4::Image::from_icon_name("application-x-executable"),
            };
            img.set_pixel_size(icon_px);
            vbox.append(&img);

            if lp_cfg.show_labels {
                let lbl = gtk4::Label::new(Some(name));
                lbl.add_css_class("launchpad-tile-label");
                lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                lbl.set_max_width_chars(14);
                lbl.set_justify(gtk4::Justification::Center);
                vbox.append(&lbl);
            }

            tile.set_child(Some(&vbox));

            let lp_w = Rc::downgrade(self);
            let id_c = id.clone();
            tile.connect_clicked(move |_| {
                if let Some(lp) = lp_w.upgrade() {
                    if let Some(entry) = lp.state.borrow().desktop_index.get(&id_c) {
                        let _ = entry.launch();
                    }
                    lp.close_animated();
                }
            });

            let rclick = gtk4::GestureClick::builder().button(3).build();
            let lp_w2 = Rc::downgrade(self);
            let id_r = id.clone();
            let name_r = name.clone();
            let tile_c = tile.clone();
            rclick.connect_pressed(move |_, _, _, _| {
                let Some(lp) = lp_w2.upgrade() else {
                    return;
                };
                let is_pinned = lp.state.borrow().config.apps.pinned.contains(&id_r);
                let pop = gtk4::Popover::new();
                pop.set_parent(&tile_c);
                pop.add_css_class("archdock-menu");
                let box_v = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                let hdr = gtk4::Label::new(Some(&name_r));
                hdr.set_halign(gtk4::Align::Start);
                hdr.add_css_class("archdock-menu-header");
                box_v.append(&hdr);

                let pin_btn = gtk4::Button::with_label(if is_pinned {
                    "Remove from Dock"
                } else {
                    "Pin to Dock"
                });
                pin_btn.add_css_class("archdock-menu-item");
                let pop_c = pop.clone();
                let st_c = lp.state.clone();
                let refresh_c = lp.on_dock_refresh.clone();
                let id_p = id_r.clone();
                pin_btn.connect_clicked(move |_| {
                    pop_c.popdown();
                    {
                        let mut st = st_c.borrow_mut();
                        if is_pinned {
                            st.config.apps.pinned.retain(|p| p != &id_p);
                        } else if !st.config.apps.pinned.contains(&id_p) {
                            st.config.apps.pinned.push(id_p.clone());
                        }
                    }
                    AppState::schedule_save(&st_c);
                    refresh_c();
                });
                box_v.append(&pin_btn);
                pop.set_child(Some(&box_v));
                pop.connect_closed(|p| {
                    let pop_c = p.clone();
                    glib::idle_add_local_once(move || pop_c.unparent());
                });
                pop.popup();
            });
            tile.add_controller(rclick);

            self.flow.insert(&tile, -1);
        }

        self.dots_box.set_visible(lp_cfg.paged_mode && pages > 1);
        if lp_cfg.paged_mode && pages > 1 {
            for p_idx in 0..pages {
                let dot = gtk4::Button::new();
                dot.add_css_class("launchpad-dot");
                if p_idx == cur_page {
                    dot.add_css_class("active");
                }
                let lp_w = Rc::downgrade(self);
                dot.connect_clicked(move |_| {
                    if let Some(lp) = lp_w.upgrade() {
                        lp.current_page.set(p_idx);
                        let q = lp.search_entry.text().to_string();
                        lp.populate(&q);
                    }
                });
                self.dots_box.append(&dot);
            }
        }
    }
}
