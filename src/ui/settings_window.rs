use crate::config::defaults::detect_default_pinned;
use crate::config::{
    is_autostart_enabled, is_hyprland_blur_enabled, is_hyprland_lua_active, parse_hex_rgb,
    set_autostart, set_hyprland_blur, ClockDateFormat, DockPosition, DockStyle, HoverAnimation,
    IndicatorStyle, IntellihideMode, LaunchAnimation, LeftClickAction, MiddleClickAction,
    ScrollAction, ThemeMode, VisibilityMode, HYPRLAND_CONF_BLUR_BODY, HYPRLAND_LUA_BLUR_BODY,
};
use crate::ui::AppState;
use gtk4::cairo;
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

pub struct SettingsWindow;

impl SettingsWindow {
    pub fn show(
        app: &gtk4::Application,
        state: Rc<RefCell<AppState>>,
        on_config_changed: Rc<dyn Fn()>,
        on_toggle_launchpad: Rc<dyn Fn()>,
    ) {
        for win in app.windows() {
            if win.widget_name() == "archdock-settings-window" {
                win.present();
                return;
            }
        }

        let window = gtk4::Window::builder()
            .application(app)
            .title("VeloDock")
            .icon_name("velodock-settings")
            .default_width(820)
            .default_height(640)
            .build();
        window.set_widget_name("archdock-settings-window");
        window.add_css_class("archdock-settings");

        let header = gtk4::HeaderBar::new();
        header.set_show_title_buttons(true);
        let htitle = gtk4::Label::new(Some("VeloDock Settings"));
        htitle.add_css_class("settings-section-title");
        header.set_title_widget(Some(&htitle));
        window.set_titlebar(Some(&header));

        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);

        let sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        sidebar.add_css_class("settings-sidebar");
        sidebar.set_size_request(215, -1);

        let brand_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        brand_row.set_margin_start(8);
        brand_row.set_margin_end(8);
        brand_row.set_margin_top(4);
        brand_row.set_margin_bottom(14);
        brand_row.set_valign(gtk4::Align::Center);

        if let Some(logo) = load_embedded_brand_icon(36) {
            brand_row.append(&logo);
        }

        let brand = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        let brand_title = gtk4::Label::new(Some("VeloDock"));
        brand_title.set_halign(gtk4::Align::Start);
        brand_title.add_css_class("settings-brand-title");
        let brand_sub = gtk4::Label::new(Some(concat!(
            "v",
            env!("CARGO_PKG_VERSION"),
            " · Wayland & X11"
        )));
        brand_sub.set_halign(gtk4::Align::Start);
        brand_sub.add_css_class("settings-brand-sub");
        brand.append(&brand_title);
        brand.append(&brand_sub);
        brand_row.append(&brand);
        sidebar.append(&brand_row);

        let stack = gtk4::Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        stack.set_transition_duration(140);

        let styles_page = build_styles_tab(state.clone(), on_config_changed.clone());
        let position_page = build_position_tab(state.clone(), on_config_changed.clone());
        let motion_page = build_motion_tab(state.clone(), on_config_changed.clone());
        let behavior_page = build_behavior_tab(state.clone(), on_config_changed.clone());
        let appearance_page = build_appearance_tab(state.clone(), on_config_changed.clone());
        let widgets_page = build_widgets_tab(state.clone(), on_config_changed.clone());
        let launchpad_page = build_launchpad_tab(
            state.clone(),
            on_config_changed.clone(),
            on_toggle_launchpad,
        );
        let apps_page = build_pinned_apps_tab(state.clone(), on_config_changed.clone());
        let general_page = build_general_tab(app.clone(), state, on_config_changed, window.clone());

        stack.add_named(&styles_page, Some("styles"));
        stack.add_named(&position_page, Some("position"));
        stack.add_named(&motion_page, Some("motion"));
        stack.add_named(&behavior_page, Some("behavior"));
        stack.add_named(&appearance_page, Some("appearance"));
        stack.add_named(&widgets_page, Some("widgets"));
        stack.add_named(&launchpad_page, Some("launchpad"));
        stack.add_named(&apps_page, Some("apps"));
        stack.add_named(&general_page, Some("general"));

        let nav_search = gtk4::SearchEntry::new();
        nav_search.set_placeholder_text(Some("Search settings…"));
        nav_search.set_margin_bottom(8);
        sidebar.append(&nav_search);

        let nav_items: [(&str, &str, &str, &str); 9] = [
            (
                "styles",
                "Dock Styles",
                "view-grid-symbolic",
                "styles preset floating attached 3d glass panel neon capsule minimal",
            ),
            (
                "position",
                "Position & Display",
                "preferences-desktop-display-symbolic",
                "position bottom top left right monitor display size spacing visibility autohide intellihide delay",
            ),
            (
                "motion",
                "Motion & Physics",
                "media-playlist-shuffle-symbolic",
                "motion physics magnification hover animation wave spotlight cascade lean orbital arc prism deck rubber flex magnet split inertia sway pulse smoothness launch bounce spin axis flip double spring press celebrate sway jolt roll",
            ),
            (
                "behavior",
                "Behavior & Icons",
                "input-mouse-symbolic",
                "behavior click left middle scroll minimize cycle new window unpinned isolate workspace indicator dot dash underline badge launchpad trash tiles",
            ),
            (
                "appearance",
                "Glass & Effects",
                "applications-graphics-symbolic",
                "appearance theme dark light opacity dynamic pywal wallpaper radius margin separator shadow sheen highlight aura tint border neon speed blur",
            ),
            (
                "widgets",
                "Widgets & Clock",
                "preferences-system-time-symbolic",
                "widgets workspace switcher media player mpris music battery cpu ram system monitor clock date 24h seconds",
            ),
            (
                "launchpad",
                "Launchpad",
                "view-app-grid-symbolic",
                "launchpad grid columns rows paged categories labels backdrop",
            ),
            (
                "apps",
                "Pinned Apps",
                "starred-symbolic",
                "pinned apps favorites reorder folder stacks",
            ),
            (
                "general",
                "General",
                "system-run-symbolic",
                "general autostart login export import config toml reset quit",
            ),
        ];

        let nav_buttons: Rc<RefCell<Vec<(String, String, gtk4::Button)>>> =
            Rc::new(RefCell::new(Vec::new()));
        for (idx, (id, label, icon_name, keywords)) in nav_items.iter().enumerate() {
            let btn = gtk4::Button::new();
            btn.add_css_class("sidebar-nav-btn");
            if idx == 0 {
                btn.add_css_class("active");
            }

            let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
            let icon = gtk4::Image::from_icon_name(icon_name);
            icon.set_pixel_size(16);
            let lbl = gtk4::Label::new(Some(label));
            lbl.set_halign(gtk4::Align::Start);
            hbox.append(&icon);
            hbox.append(&lbl);
            btn.set_child(Some(&hbox));

            let stack_c = stack.clone();
            let btns_c = nav_buttons.clone();
            let target_id = id.to_string();
            btn.connect_clicked(move |_| {
                stack_c.set_visible_child_name(&target_id);
                for (bid, _, b) in btns_c.borrow().iter() {
                    if bid == &target_id {
                        b.add_css_class("active");
                    } else {
                        b.remove_css_class("active");
                    }
                }
            });

            nav_buttons
                .borrow_mut()
                .push((id.to_string(), (*keywords).to_string(), btn.clone()));
            sidebar.append(&btn);
        }

        {
            let btns_c = nav_buttons.clone();
            let stack_c = stack.clone();
            nav_search.connect_search_changed(move |entry| {
                let q = entry.text().trim().to_lowercase();
                let mut first_match: Option<String> = None;
                for (bid, kw, btn) in btns_c.borrow().iter() {
                    let matched = q.is_empty() || kw.contains(&q) || bid.contains(&q);
                    btn.set_visible(matched);
                    if matched && first_match.is_none() {
                        first_match = Some(bid.clone());
                    }
                }
                if !q.is_empty() {
                    if let Some(target) = first_match {
                        stack_c.set_visible_child_name(&target);
                        for (bid, _, b) in btns_c.borrow().iter() {
                            if bid == &target {
                                b.add_css_class("active");
                            } else {
                                b.remove_css_class("active");
                            }
                        }
                    }
                }
            });
        }

        root.append(&sidebar);
        root.append(&stack);
        window.set_child(Some(&root));
        window.present();
    }
}

fn load_embedded_brand_icon(size: i32) -> Option<gtk4::Image> {
    const SVG_BYTES: &[u8] = include_bytes!("../../assets/icons/velodock.svg");
    let loader = gtk4::gdk_pixbuf::PixbufLoader::new();
    loader.set_size(size, size);
    loader.write(SVG_BYTES).ok()?;
    loader.close().ok()?;
    let pb = loader.pixbuf()?;
    let texture = gdk::Texture::for_pixbuf(&pb);
    let img = gtk4::Image::from_paintable(Some(&texture));
    img.set_pixel_size(size);
    Some(img)
}

fn page_container(title: &str, subtitle: &str) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    content.set_margin_top(20);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);

    let t = gtk4::Label::new(Some(title));
    t.set_halign(gtk4::Align::Start);
    t.add_css_class("settings-page-title");
    content.append(&t);

    let s = gtk4::Label::new(Some(subtitle));
    s.set_halign(gtk4::Align::Start);
    s.add_css_class("settings-muted");
    s.set_margin_bottom(4);
    content.append(&s);
    content
}

fn card() -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    b.add_css_class("settings-card");
    b
}

fn row_with_widget(
    title: &str,
    subtitle: Option<&str>,
    widget: &impl IsA<gtk4::Widget>,
) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row.set_valign(gtk4::Align::Center);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text_box.set_hexpand(true);
    let lbl = gtk4::Label::new(Some(title));
    lbl.set_halign(gtk4::Align::Start);
    text_box.append(&lbl);
    if let Some(sub) = subtitle {
        let sub_lbl = gtk4::Label::new(Some(sub));
        sub_lbl.set_halign(gtk4::Align::Start);
        sub_lbl.add_css_class("settings-muted");
        text_box.append(&sub_lbl);
    }

    row.append(&text_box);
    row.append(widget);
    row
}

fn notify_change(state: &Rc<RefCell<AppState>>, on_changed: &Rc<dyn Fn()>) {
    state.borrow_mut().config.clamp();
    AppState::schedule_save(state);
    on_changed();
}

fn build_styles_tab(
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "Dock Styles",
        "Pick a visual theme or 1-click preset — updates your live dock immediately",
    );

    let preset_card = card();
    let p_title = gtk4::Label::new(Some("Full Look Presets"));
    p_title.set_halign(gtk4::Align::Start);
    p_title.add_css_class("settings-section-title");
    preset_card.append(&p_title);

    let preset_row = gtk4::FlowBox::new();
    preset_row.set_max_children_per_line(3);
    preset_row.set_min_children_per_line(2);
    preset_row.set_row_spacing(8);
    preset_row.set_column_spacing(8);
    preset_row.set_selection_mode(gtk4::SelectionMode::None);

    let presets: [(&str, fn(&mut crate::config::Config)); 6] = [
        ("Frosted Clean", |c| {
            c.appearance.style = DockStyle::Floating;
            c.appearance.theme = ThemeMode::Auto;
            c.appearance.opacity = 0.72;
            c.appearance.corner_radius = 20;
            c.appearance.border_width = 1.0;
            c.appearance.custom_tint = None;
            c.appearance.custom_indicator = None;
            c.appearance.show_separators = true;
            c.dock.hover_animation = HoverAnimation::Wave;
            c.dock.launch_animation = LaunchAnimation::Bounce;
            c.dock.indicator_style = IndicatorStyle::Dot;
            c.dock.animation_smoothness = 1.0;
        }),
        ("Prism 3D", |c| {
            c.appearance.style = DockStyle::Glass3D;
            c.appearance.theme = ThemeMode::Dark;
            c.appearance.opacity = 0.68;
            c.appearance.adaptive_aura = true;
            c.appearance.cursor_sheen = true;
            c.dock.hover_animation = HoverAnimation::PrismDeck;
            c.dock.launch_animation = LaunchAnimation::AxisFlip;
            c.dock.indicator_style = IndicatorStyle::MultiDots;
            c.dock.animation_smoothness = 1.05;
        }),
        ("Neon Arcade", |c| {
            c.appearance.style = DockStyle::CyberNeon;
            c.appearance.theme = ThemeMode::Dark;
            c.appearance.opacity = 0.80;
            c.appearance.neon_speed = 1.4;
            c.appearance.border_width = 2.0;
            c.appearance.cursor_sheen = true;
            c.appearance.icon_shadow = true;
            c.dock.hover_animation = HoverAnimation::PulseWave;
            c.dock.launch_animation = LaunchAnimation::DoublePulse;
            c.dock.indicator_style = IndicatorStyle::Dash;
        }),
        ("Orbital Dark", |c| {
            c.appearance.style = DockStyle::Capsule;
            c.appearance.theme = ThemeMode::Dark;
            c.appearance.opacity = 0.64;
            c.appearance.dynamic_opacity = true;
            c.appearance.show_separators = false;
            c.dock.hover_animation = HoverAnimation::OrbitalArc;
            c.dock.launch_animation = LaunchAnimation::SpringPress;
            c.dock.indicator_style = IndicatorStyle::Underline;
        }),
        ("Rubber Playful", |c| {
            c.appearance.style = DockStyle::Floating;
            c.appearance.theme = ThemeMode::Auto;
            c.appearance.opacity = 0.74;
            c.appearance.adaptive_aura = true;
            c.dock.hover_animation = HoverAnimation::RubberFlex;
            c.dock.launch_animation = LaunchAnimation::PopCelebrate;
            c.dock.indicator_style = IndicatorStyle::Dot;
            c.dock.animation_smoothness = 0.85;
        }),
        ("Minimal Bar", |c| {
            c.appearance.style = DockStyle::Panel;
            c.appearance.theme = ThemeMode::Dark;
            c.appearance.opacity = 0.78;
            c.appearance.show_separators = true;
            c.dock.hover_animation = HoverAnimation::Spotlight;
            c.dock.launch_animation = LaunchAnimation::Bounce;
            c.dock.indicator_style = IndicatorStyle::Underline;
        }),
    ];
    for (label, apply_fn) in presets {
        let btn = gtk4::Button::with_label(label);
        let st_c = state.clone();
        let cb_c = on_changed.clone();
        btn.connect_clicked(move |_| {
            apply_fn(&mut st_c.borrow_mut().config);
            notify_change(&st_c, &cb_c);
        });
        preset_row.insert(&btn, -1);
    }
    preset_card.append(&preset_row);
    content.append(&preset_card);

    let flow = gtk4::FlowBox::new();
    flow.set_max_children_per_line(2);
    flow.set_min_children_per_line(2);
    flow.set_row_spacing(12);
    flow.set_column_spacing(12);
    flow.set_homogeneous(true);
    flow.set_selection_mode(gtk4::SelectionMode::None);

    let current_style = state.borrow().config.appearance.style;
    let cards: Rc<RefCell<Vec<(DockStyle, gtk4::Button)>>> = Rc::new(RefCell::new(Vec::new()));

    for &style in DockStyle::ALL {
        let btn = gtk4::Button::new();
        btn.add_css_class("style-card");
        if style == current_style {
            btn.add_css_class("selected");
        }

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let preview = gtk4::DrawingArea::new();
        preview.set_content_width(220);
        preview.set_content_height(64);
        preview.set_draw_func(move |_, cr, w, h| {
            draw_style_preview(cr, w as f64, h as f64, style);
        });
        vbox.append(&preview);

        let name_lbl = gtk4::Label::new(Some(style.display_name()));
        name_lbl.set_halign(gtk4::Align::Start);
        name_lbl.add_css_class("settings-section-title");
        vbox.append(&name_lbl);

        let sub_lbl = gtk4::Label::new(Some(style.subtitle()));
        sub_lbl.set_halign(gtk4::Align::Start);
        sub_lbl.add_css_class("settings-muted");
        vbox.append(&sub_lbl);

        btn.set_child(Some(&vbox));

        let st_c = state.clone();
        let cb_c = on_changed.clone();
        let cards_c = cards.clone();
        btn.connect_clicked(move |_| {
            st_c.borrow_mut().config.appearance.style = style;
            notify_change(&st_c, &cb_c);
            for (s, b) in cards_c.borrow().iter() {
                if *s == style {
                    b.add_css_class("selected");
                } else {
                    b.remove_css_class("selected");
                }
            }
        });

        cards.borrow_mut().push((style, btn.clone()));
        flow.insert(&btn, -1);
    }

    content.append(&flow);
    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}

fn draw_style_preview(cr: &cairo::Context, w: f64, h: f64, style: DockStyle) {

    let bg = cairo::LinearGradient::new(0.0, 0.0, w, h);
    bg.add_color_stop_rgb(0.0, 0.14, 0.20, 0.34);
    bg.add_color_stop_rgb(0.6, 0.28, 0.19, 0.36);
    bg.add_color_stop_rgb(1.0, 0.16, 0.26, 0.32);
    preview_rounded_rect(cr, 0.0, 0.0, w, h, 8.0);
    let _ = cr.set_source(&bg);
    let _ = cr.fill();

    let dw = w * 0.78;
    let dh = 26.0;
    let dx = (w - dw) * 0.5;
    let dy = h - dh - 6.0;

    match style {
        DockStyle::Floating => {
            preview_rounded_rect(cr, dx, dy, dw, dh, 9.0);
            cr.set_source_rgba(0.14, 0.14, 0.18, 0.78);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.26);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        }
        DockStyle::Attached => {
            let ay = h - dh;
            preview_rounded_rect(cr, dx, ay, dw, dh + 8.0, 10.0);
            cr.set_source_rgba(0.16, 0.14, 0.12, 0.80);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.20);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        }
        DockStyle::Glass3D => {
            let top_y = h - 15.0;
            let bot_y = h - 3.0;
            cr.new_sub_path();
            cr.move_to(dx + 10.0, top_y);
            cr.line_to(dx + dw - 10.0, top_y);
            cr.line_to(dx + dw + 4.0, bot_y);
            cr.line_to(dx - 4.0, bot_y);
            cr.close_path();
            cr.set_source_rgba(0.84, 0.88, 0.96, 0.52);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.65);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
            cr.rectangle(dx - 4.0, bot_y, dw + 8.0, 3.0);
            cr.set_source_rgba(0.95, 0.97, 1.0, 0.82);
            let _ = cr.fill();
        }
        DockStyle::Panel => {
            cr.rectangle(0.0, h - dh, w, dh);
            cr.set_source_rgba(0.12, 0.12, 0.16, 0.82);
            let _ = cr.fill();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.22);
            cr.move_to(0.0, h - dh + 0.5);
            cr.line_to(w, h - dh + 0.5);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        }
        DockStyle::CyberNeon => {
            preview_rounded_rect(cr, dx, dy, dw, dh, 9.0);
            cr.set_source_rgba(0.04, 0.05, 0.09, 0.90);
            let _ = cr.fill_preserve();
            let neon = cairo::LinearGradient::new(dx, dy, dx + dw, dy + dh);
            neon.add_color_stop_rgb(0.0, 0.0, 0.96, 1.0);
            neon.add_color_stop_rgb(0.5, 0.62, 0.25, 1.0);
            neon.add_color_stop_rgb(1.0, 1.0, 0.18, 0.55);
            let _ = cr.set_source(&neon);
            cr.set_line_width(1.8);
            let _ = cr.stroke();
        }
        DockStyle::Capsule => {
            preview_rounded_rect(cr, dx, dy, dw, dh, dh * 0.5);
            cr.set_source_rgba(0.05, 0.05, 0.07, 0.92);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.26);
            cr.set_line_width(1.2);
            let _ = cr.stroke();
        }
        DockStyle::SplitIslands => {
            let left_w = dw * 0.62;
            let right_w = dw * 0.28;
            let gap_w = dw - left_w - right_w;
            for (ix, iw) in [(dx, left_w), (dx + left_w + gap_w, right_w)] {
                preview_rounded_rect(cr, ix, dy, iw, dh, 9.0);
                cr.set_source_rgba(0.14, 0.14, 0.18, 0.78);
                let _ = cr.fill_preserve();
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.26);
                cr.set_line_width(1.0);
                let _ = cr.stroke();
            }
        }
        DockStyle::BaselineBar => {
            preview_rounded_rect(cr, dx + 6.0, h - 9.0, dw - 12.0, 4.5, 2.2);
            cr.set_source_rgba(0.18, 0.20, 0.26, 0.86);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.42);
            cr.set_line_width(0.9);
            let _ = cr.stroke();
        }
        DockStyle::Aurora => {
            preview_rounded_rect(cr, dx, dy, dw, dh, 9.0);
            cr.set_source_rgba(0.08, 0.09, 0.13, 0.86);
            let _ = cr.fill_preserve();
            let wash = cairo::LinearGradient::new(dx, dy, dx + dw, dy);
            wash.add_color_stop_rgba(0.0, 0.28, 0.72, 0.68, 0.30);
            wash.add_color_stop_rgba(0.5, 0.44, 0.42, 0.74, 0.26);
            wash.add_color_stop_rgba(1.0, 0.66, 0.40, 0.70, 0.30);
            let _ = cr.set_source(&wash);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.24);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        }
        DockStyle::EdgeRail => {
            cr.rectangle(0.0, h - dh, w, dh);
            cr.set_source_rgba(0.11, 0.11, 0.15, 0.86);
            let _ = cr.fill();
            let gleam = cairo::LinearGradient::new(0.0, h - dh + 0.8, w, h - dh + 0.8);
            gleam.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.08);
            gleam.add_color_stop_rgba(0.5, 0.38, 0.76, 1.0, 0.72);
            gleam.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.08);
            let _ = cr.set_source(&gleam);
            cr.move_to(0.0, h - dh + 0.8);
            cr.line_to(w, h - dh + 0.8);
            cr.set_line_width(1.4);
            let _ = cr.stroke();
        }
        DockStyle::Invisible => {}
    }

    let icon_colors = [
        (0.99, 0.36, 0.34),
        (1.00, 0.64, 0.12),
        (0.24, 0.80, 0.45),
        (0.14, 0.56, 1.00),
        (0.66, 0.36, 0.98),
    ];
    let icon_sz = 16.0;
    let gap = 8.0;
    let row_w = icon_colors.len() as f64 * icon_sz + (icon_colors.len() - 1) as f64 * gap;
    let start_x = (w - row_w) * 0.5;
    let icon_y = match style {
        DockStyle::Attached | DockStyle::Panel | DockStyle::Glass3D => h - 22.0,
        _ => dy + 5.0,
    };
    for (i, &(r, g, b)) in icon_colors.iter().enumerate() {
        let sz = if i == 2 { icon_sz * 1.25 } else { icon_sz };
        let ix = start_x + i as f64 * (icon_sz + gap) - (sz - icon_sz) * 0.5;
        let iy = icon_y - (sz - icon_sz);
        preview_rounded_rect(cr, ix, iy, sz, sz, 4.5);
        cr.set_source_rgb(r, g, b);
        let _ = cr.fill();
    }
}

fn preview_rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI * 0.5, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI * 0.5);
    cr.arc(x + r, y + h - r, r, PI * 0.5, PI);
    cr.arc(x + r, y + r, r, PI, PI * 1.5);
    cr.close_path();
}

fn build_position_tab(
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "Position & Display",
        "Choose dock edge, monitor, icon sizing, and smart auto-hide behavior",
    );
    let cfg = state.borrow().config.dock.clone();

    let pos_card = card();
    let pos_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    let btn_left = gtk4::ToggleButton::with_label("Left");
    let btn_bottom = gtk4::ToggleButton::with_label("Bottom");
    let btn_top = gtk4::ToggleButton::with_label("Top");
    let btn_right = gtk4::ToggleButton::with_label("Right");
    btn_bottom.set_group(Some(&btn_left));
    btn_top.set_group(Some(&btn_left));
    btn_right.set_group(Some(&btn_left));
    match cfg.position {
        DockPosition::Left => btn_left.set_active(true),
        DockPosition::Bottom => btn_bottom.set_active(true),
        DockPosition::Top => btn_top.set_active(true),
        DockPosition::Right => btn_right.set_active(true),
    }
    for (btn, pos) in [
        (&btn_left, DockPosition::Left),
        (&btn_bottom, DockPosition::Bottom),
        (&btn_top, DockPosition::Top),
        (&btn_right, DockPosition::Right),
    ] {
        let st = state.clone();
        let cb = on_changed.clone();
        btn.connect_toggled(move |b| {
            if b.is_active() {
                st.borrow_mut().config.dock.position = pos;
                notify_change(&st, &cb);
            }
        });
        pos_box.append(btn);
    }
    pos_card.append(&row_with_widget("Position on screen", None, &pos_box));

    let mut monitor_labels = vec!["Primary Monitor".to_string()];
    let detected_monitors = crate::platform::surface::list_monitor_names();
    monitor_labels.extend(detected_monitors.clone());
    let monitor_refs: Vec<&str> = monitor_labels.iter().map(String::as_str).collect();
    let mon_dd = gtk4::DropDown::from_strings(&monitor_refs);
    let selected_mon = cfg
        .monitor_name
        .as_deref()
        .and_then(|name| detected_monitors.iter().position(|m| m == name))
        .map(|i| (i + 1) as u32)
        .unwrap_or(0);
    mon_dd.set_selected(selected_mon);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        let names = detected_monitors;
        mon_dd.connect_selected_notify(move |dd| {
            let sel = dd.selected() as usize;
            st.borrow_mut().config.dock.monitor_name = if sel == 0 {
                None
            } else {
                names.get(sel - 1).cloned()
            };
            notify_change(&st, &cb);
        });
    }
    pos_card.append(&row_with_widget(
        "Display Monitor",
        Some("Choose which monitor hosts the dock"),
        &mon_dd,
    ));

    let size_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 32.0, 80.0, 2.0);
    size_scale.set_value(cfg.icon_size as f64);
    size_scale.set_size_request(210, -1);
    size_scale.set_draw_value(true);
    size_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        size_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.dock.icon_size = s.value().round() as u32;
            notify_change(&st, &cb);
        });
    }
    pos_card.append(&row_with_widget("Icon Size (px)", None, &size_scale));

    let spacing_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 2.0, 24.0, 1.0);
    spacing_scale.set_value(cfg.icon_spacing as f64);
    spacing_scale.set_size_request(210, -1);
    spacing_scale.set_draw_value(true);
    spacing_scale.set_digits(0);
    spacing_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        spacing_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.dock.icon_spacing = s.value().round() as u32;
            notify_change(&st, &cb);
        });
    }
    pos_card.append(&row_with_widget("Icon Spacing (px)", None, &spacing_scale));
    content.append(&pos_card);

    let vis_card = card();
    let v_title = gtk4::Label::new(Some("Visibility & Intellihide"));
    v_title.set_halign(gtk4::Align::Start);
    v_title.add_css_class("settings-section-title");
    vis_card.append(&v_title);

    let vis_dropdown =
        gtk4::DropDown::from_strings(&["Always Visible", "Auto-hide", "Intellihide"]);
    vis_dropdown.set_selected(match cfg.visibility {
        VisibilityMode::Always => 0,
        VisibilityMode::AutoHide => 1,
        VisibilityMode::Intellihide => 2,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        vis_dropdown.connect_selected_notify(move |dd| {
            let mode = match dd.selected() {
                1 => VisibilityMode::AutoHide,
                2 => VisibilityMode::Intellihide,
                _ => VisibilityMode::Always,
            };
            st.borrow_mut().config.dock.visibility = mode;
            notify_change(&st, &cb);
        });
    }
    vis_card.append(&row_with_widget("Visibility Mode", None, &vis_dropdown));

    let ih_dd = gtk4::DropDown::from_strings(&[
        "All Overlapping Windows",
        "Focused Window Only",
        "Maximized Windows Only",
    ]);
    ih_dd.set_selected(match cfg.intellihide_mode {
        IntellihideMode::AllWindows => 0,
        IntellihideMode::FocusedOnly => 1,
        IntellihideMode::MaximizedOnly => 2,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        ih_dd.connect_selected_notify(move |dd| {
            st.borrow_mut().config.dock.intellihide_mode = match dd.selected() {
                1 => IntellihideMode::FocusedOnly,
                2 => IntellihideMode::MaximizedOnly,
                _ => IntellihideMode::AllWindows,
            };
            notify_change(&st, &cb);
        });
    }
    vis_card.append(&row_with_widget(
        "Intellihide Trigger",
        Some("Controls which windows cause Intellihide to tuck the dock away"),
        &ih_dd,
    ));

    let delay_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1000.0, 20.0);
    delay_scale.set_value(cfg.autohide_delay_ms as f64);
    delay_scale.set_size_request(210, -1);
    delay_scale.set_draw_value(true);
    delay_scale.set_digits(0);
    delay_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        delay_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.dock.autohide_delay_ms = s.value().round() as u32;
            notify_change(&st, &cb);
        });
    }
    vis_card.append(&row_with_widget("Auto-hide delay (ms)", None, &delay_scale));

    let peek_sw = gtk4::Switch::new();
    peek_sw.set_active(cfg.autohide_peek);
    peek_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        peek_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.dock.autohide_peek = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    vis_card.append(&row_with_widget(
        "Auto-hide Edge Peek Strip",
        Some("Leaves a subtle 4px luminous strip along the edge when tucked away"),
        &peek_sw,
    ));
    content.append(&vis_card);

    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}

fn build_motion_tab(
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "Motion & Physics",
        "Customize hover magnification physics, spring smoothness, and click launch effects",
    );
    let cfg = state.borrow().config.dock.clone();

    let hover_card = card();
    let h_title = gtk4::Label::new(Some("Hover Physics"));
    h_title.set_halign(gtk4::Align::Start);
    h_title.add_css_class("settings-section-title");
    hover_card.append(&h_title);

    let mag_switch = gtk4::Switch::new();
    mag_switch.set_active(cfg.magnification);
    mag_switch.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        mag_switch.connect_state_set(move |_, active| {
            st.borrow_mut().config.dock.magnification = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    hover_card.append(&row_with_widget(
        "Enable Hover Magnification",
        Some("Dynamic zoom and motion response under the pointer"),
        &mag_switch,
    ));

    let hover_anim_dd = gtk4::DropDown::from_strings(&[
        "Original Wave",
        "Spotlight Peak",
        "Cascade Lean",
        "Orbital Arc",
        "Prism Deck",
        "Rubber Flex",
        "Magnet Split",
        "Inertia Sway",
        "Pulse Wave",
    ]);
    hover_anim_dd.set_selected(match cfg.hover_animation {
        HoverAnimation::Wave => 0,
        HoverAnimation::Spotlight => 1,
        HoverAnimation::CascadeLean => 2,
        HoverAnimation::OrbitalArc => 3,
        HoverAnimation::PrismDeck => 4,
        HoverAnimation::RubberFlex => 5,
        HoverAnimation::MagnetSplit => 6,
        HoverAnimation::InertiaSway => 7,
        HoverAnimation::PulseWave => 8,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        hover_anim_dd.connect_selected_notify(move |dd| {
            st.borrow_mut().config.dock.hover_animation = match dd.selected() {
                1 => HoverAnimation::Spotlight,
                2 => HoverAnimation::CascadeLean,
                3 => HoverAnimation::OrbitalArc,
                4 => HoverAnimation::PrismDeck,
                5 => HoverAnimation::RubberFlex,
                6 => HoverAnimation::MagnetSplit,
                7 => HoverAnimation::InertiaSway,
                8 => HoverAnimation::PulseWave,
                _ => HoverAnimation::Wave,
            };
            notify_change(&st, &cb);
        });
    }
    hover_card.append(&row_with_widget(
        "Hover Animation",
        Some("9 distinct physics & geometric hover motions"),
        &hover_anim_dd,
    ));

    let mag_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 1.0, 2.2, 0.05);
    mag_scale.set_value(cfg.magnification_scale);
    mag_scale.set_size_request(210, -1);
    mag_scale.set_draw_value(true);
    mag_scale.set_digits(2);
    mag_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        mag_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.dock.magnification_scale = s.value();
            notify_change(&st, &cb);
        });
    }
    hover_card.append(&row_with_widget("Magnification Scale", None, &mag_scale));

    let spread_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 1.6, 4.0, 0.1);
    spread_scale.set_value(cfg.magnification_spread);
    spread_scale.set_size_request(210, -1);
    spread_scale.set_draw_value(true);
    spread_scale.set_digits(1);
    spread_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        spread_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.dock.magnification_spread = s.value();
            notify_change(&st, &cb);
        });
    }
    hover_card.append(&row_with_widget(
        "Wave Spread",
        Some("How many neighbor icons participate in the motion"),
        &spread_scale,
    ));

    let lift_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 48.0, 2.0);
    lift_scale.set_value(cfg.hover_lift);
    lift_scale.set_size_request(210, -1);
    lift_scale.set_draw_value(true);
    lift_scale.set_digits(0);
    lift_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        lift_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.dock.hover_lift = s.value();
            notify_change(&st, &cb);
        });
    }
    hover_card.append(&row_with_widget(
        "Hover Lift (px)",
        Some("Float zoomed icons cleanly above the dock tray"),
        &lift_scale,
    ));

    let smooth_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.3, 2.0, 0.05);
    smooth_scale.set_value(cfg.animation_smoothness);
    smooth_scale.set_size_request(210, -1);
    smooth_scale.set_draw_value(true);
    smooth_scale.set_digits(2);
    smooth_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        smooth_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.dock.animation_smoothness = s.value();
            notify_change(&st, &cb);
        });
    }
    hover_card.append(&row_with_widget(
        "Spring Smoothness",
        Some("Lower = snappy, higher = silky glide"),
        &smooth_scale,
    ));
    content.append(&hover_card);

    let launch_card = card();
    let l_title = gtk4::Label::new(Some("Launch & Click Effects"));
    l_title.set_halign(gtk4::Align::Start);
    l_title.add_css_class("settings-section-title");
    launch_card.append(&l_title);

    let anim_sw = gtk4::Switch::new();
    anim_sw.set_active(cfg.animate_launch);
    anim_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        anim_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.dock.animate_launch = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    launch_card.append(&row_with_widget(
        "Animate Opening Applications",
        Some("Play an effect when clicking an app to launch"),
        &anim_sw,
    ));

    let launch_anim_dd = gtk4::DropDown::from_strings(&[
        "Spring Bounce",
        "Elastic Pop",
        "Glow Pulse",
        "Playful Wiggle",
        "Smooth 360° Spin",
        "Axis Flip",
        "Double Pulse",
        "Spring Press",
        "Pop Celebrate",
        "Damped Sway",
        "Quick Jolt",
        "Arc Roll",
    ]);
    launch_anim_dd.set_selected(match cfg.launch_animation {
        LaunchAnimation::Bounce => 0,
        LaunchAnimation::ElasticPop => 1,
        LaunchAnimation::PulseGlow => 2,
        LaunchAnimation::Wiggle => 3,
        LaunchAnimation::Spin360 => 4,
        LaunchAnimation::AxisFlip => 5,
        LaunchAnimation::DoublePulse => 6,
        LaunchAnimation::SpringPress => 7,
        LaunchAnimation::PopCelebrate => 8,
        LaunchAnimation::DampedSway => 9,
        LaunchAnimation::QuickJolt => 10,
        LaunchAnimation::ArcRoll => 11,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        launch_anim_dd.connect_selected_notify(move |dd| {
            st.borrow_mut().config.dock.launch_animation = match dd.selected() {
                1 => LaunchAnimation::ElasticPop,
                2 => LaunchAnimation::PulseGlow,
                3 => LaunchAnimation::Wiggle,
                4 => LaunchAnimation::Spin360,
                5 => LaunchAnimation::AxisFlip,
                6 => LaunchAnimation::DoublePulse,
                7 => LaunchAnimation::SpringPress,
                8 => LaunchAnimation::PopCelebrate,
                9 => LaunchAnimation::DampedSway,
                10 => LaunchAnimation::QuickJolt,
                11 => LaunchAnimation::ArcRoll,
                _ => LaunchAnimation::Bounce,
            };
            notify_change(&st, &cb);
        });
    }
    launch_card.append(&row_with_widget("Launch Animation", None, &launch_anim_dd));

    let bounce_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.4, 1.8, 0.1);
    bounce_scale.set_value(cfg.bounce_intensity);
    bounce_scale.set_size_request(210, -1);
    bounce_scale.set_draw_value(true);
    bounce_scale.set_digits(1);
    bounce_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        bounce_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.dock.bounce_intensity = s.value();
            notify_change(&st, &cb);
        });
    }
    launch_card.append(&row_with_widget(
        "Effect Intensity",
        Some("Amplitude of launch animations"),
        &bounce_scale,
    ));
    content.append(&launch_card);

    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}

fn build_behavior_tab(
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "Behavior & Icons",
        "Mouse click actions, workspace isolation, dock items, and running indicators",
    );
    let cfg = state.borrow().config.dock.clone();
    let app_cfg = state.borrow().config.appearance.clone();

    let mouse_card = card();
    let m_title = gtk4::Label::new(Some("Click & Scroll Actions"));
    m_title.set_halign(gtk4::Align::Start);
    m_title.add_css_class("settings-section-title");
    mouse_card.append(&m_title);

    let left_dd = gtk4::DropDown::from_strings(&[
        "Focus or Cycle Windows",
        "Focus, Cycle or Minimize",
        "Open New Window",
    ]);
    let eff_left = if cfg.left_click == LeftClickAction::FocusOrCycle && cfg.click_minimize {
        LeftClickAction::FocusCycleOrMinimize
    } else {
        cfg.left_click
    };
    left_dd.set_selected(match eff_left {
        LeftClickAction::FocusOrCycle => 0,
        LeftClickAction::FocusCycleOrMinimize => 1,
        LeftClickAction::FocusOrNewWindow => 2,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        left_dd.connect_selected_notify(move |dd| {
            let mut s = st.borrow_mut();
            s.config.dock.left_click = match dd.selected() {
                1 => LeftClickAction::FocusCycleOrMinimize,
                2 => LeftClickAction::FocusOrNewWindow,
                _ => LeftClickAction::FocusOrCycle,
            };
            s.config.dock.click_minimize = dd.selected() == 1;
            drop(s);
            notify_change(&st, &cb);
        });
    }
    mouse_card.append(&row_with_widget(
        "Left-click running app",
        Some("Action when clicking an already-open application"),
        &left_dd,
    ));

    let mid_dd = gtk4::DropDown::from_strings(&["Open New Window", "Quit Application", "Do Nothing"]);
    mid_dd.set_selected(match cfg.middle_click {
        MiddleClickAction::NewWindow => 0,
        MiddleClickAction::Quit => 1,
        MiddleClickAction::None => 2,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        mid_dd.connect_selected_notify(move |dd| {
            st.borrow_mut().config.dock.middle_click = match dd.selected() {
                1 => MiddleClickAction::Quit,
                2 => MiddleClickAction::None,
                _ => MiddleClickAction::NewWindow,
            };
            notify_change(&st, &cb);
        });
    }
    mouse_card.append(&row_with_widget("Middle-click app icon", None, &mid_dd));

    let scroll_dd = gtk4::DropDown::from_strings(&["Cycle Open Windows", "Do Nothing"]);
    scroll_dd.set_selected(match cfg.scroll_action {
        ScrollAction::CycleWindows => 0,
        ScrollAction::None => 1,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        scroll_dd.connect_selected_notify(move |dd| {
            st.borrow_mut().config.dock.scroll_action = match dd.selected() {
                1 => ScrollAction::None,
                _ => ScrollAction::CycleWindows,
            };
            notify_change(&st, &cb);
        });
    }
    mouse_card.append(&row_with_widget("Scroll wheel on app icon", None, &scroll_dd));
    content.append(&mouse_card);

    let items_card = card();
    let i_title = gtk4::Label::new(Some("Dock Items & Filtering"));
    i_title.set_halign(gtk4::Align::Start);
    i_title.add_css_class("settings-section-title");
    items_card.append(&i_title);

    let add_item_toggle = |title: &str,
                           sub: Option<&str>,
                           initial: bool,
                           setter: Box<dyn Fn(&mut crate::config::DockConfig, bool)>| {
        let sw = gtk4::Switch::new();
        sw.set_active(initial);
        sw.set_valign(gtk4::Align::Center);
        let st = state.clone();
        let cb = on_changed.clone();
        sw.connect_state_set(move |_, active| {
            setter(&mut st.borrow_mut().config.dock, active);
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
        items_card.append(&row_with_widget(title, sub, &sw));
    };

    add_item_toggle(
        "Show unpinned running apps",
        Some("Append open applications that are not pinned"),
        cfg.show_unpinned,
        Box::new(|d, v| d.show_unpinned = v),
    );
    add_item_toggle(
        "Isolate active workspace",
        Some("Show running apps and indicators only for the current workspace"),
        cfg.isolate_workspaces,
        Box::new(|d, v| d.isolate_workspaces = v),
    );
    add_item_toggle(
        "Show Launchpad icon",
        Some("Drag the Launchpad icon directly on the dock to reposition it"),
        cfg.show_launchpad,
        Box::new(|d, v| d.show_launchpad = v),
    );
    add_item_toggle(
        "Show Trash icon",
        None,
        cfg.show_trash,
        Box::new(|d, v| d.show_trash = v),
    );
    add_item_toggle(
        "Show Open Window Tiles on Dock",
        Some("Display individual mini window cards next to running apps"),
        cfg.show_window_tiles,
        Box::new(|d, v| d.show_window_tiles = v),
    );
    content.append(&items_card);

    let ind_card = card();
    let ind_title = gtk4::Label::new(Some("Running Indicators"));
    ind_title.set_halign(gtk4::Align::Start);
    ind_title.add_css_class("settings-section-title");
    ind_card.append(&ind_title);

    let show_ind_sw = gtk4::Switch::new();
    show_ind_sw.set_active(cfg.show_indicators);
    show_ind_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        show_ind_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.dock.show_indicators = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    ind_card.append(&row_with_widget("Show running indicators", None, &show_ind_sw));

    let ind_style_dd = gtk4::DropDown::from_strings(&[
        "Dot",
        "Pill Dash",
        "Multi-Dots (1–4 windows)",
        "Underline Bar",
    ]);
    ind_style_dd.set_selected(match cfg.indicator_style {
        IndicatorStyle::Dot => 0,
        IndicatorStyle::Dash => 1,
        IndicatorStyle::MultiDots => 2,
        IndicatorStyle::Underline => 3,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        ind_style_dd.connect_selected_notify(move |dd| {
            st.borrow_mut().config.dock.indicator_style = match dd.selected() {
                1 => IndicatorStyle::Dash,
                2 => IndicatorStyle::MultiDots,
                3 => IndicatorStyle::Underline,
                _ => IndicatorStyle::Dot,
            };
            notify_change(&st, &cb);
        });
    }
    ind_card.append(&row_with_widget("Indicator Style", None, &ind_style_dd));

    let ind_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let ind_dialog = gtk4::ColorDialog::new();
    let ind_btn = gtk4::ColorDialogButton::new(Some(ind_dialog));
    if let Some((r, g, b)) = app_cfg.custom_indicator.as_deref().and_then(parse_hex_rgb) {
        ind_btn.set_rgba(&gdk::RGBA::new(r as f32, g as f32, b as f32, 1.0));
    } else {
        ind_btn.set_rgba(&gdk::RGBA::new(0.22, 0.74, 0.97, 1.0));
    }
    {
        let st = state.clone();
        let cb = on_changed.clone();
        ind_btn.connect_rgba_notify(move |b| {
            let c = b.rgba();
            let hex = format!(
                "#{:02x}{:02x}{:02x}",
                (c.red() * 255.0).round() as u8,
                (c.green() * 255.0).round() as u8,
                (c.blue() * 255.0).round() as u8
            );
            st.borrow_mut().config.appearance.custom_indicator = Some(hex);
            notify_change(&st, &cb);
        });
    }
    let ind_reset = gtk4::Button::with_label("Auto");
    {
        let st = state.clone();
        let cb = on_changed.clone();
        ind_reset.connect_clicked(move |_| {
            st.borrow_mut().config.appearance.custom_indicator = None;
            notify_change(&st, &cb);
        });
    }
    ind_box.append(&ind_btn);
    ind_box.append(&ind_reset);
    ind_card.append(&row_with_widget("Indicator Color", None, &ind_box));

    let badge_sw = gtk4::Switch::new();
    badge_sw.set_active(cfg.show_window_badges);
    badge_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        badge_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.dock.show_window_badges = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    ind_card.append(&row_with_widget(
        "Window count badges (2+ windows)",
        Some("Compact numeric pill on icons with multiple open windows"),
        &badge_sw,
    ));
    content.append(&ind_card);

    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}

fn build_widgets_tab(
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "Widgets & Clock",
        "Optional embedded modules inside the dock tray (all off by default)",
    );
    let cfg = state.borrow().config.dock.clone();

    let widgets_card = card();
    let w_title = gtk4::Label::new(Some("Dock Modules"));
    w_title.set_halign(gtk4::Align::Start);
    w_title.add_css_class("settings-section-title");
    widgets_card.append(&w_title);

    let add_widget_toggle = |title: &str,
                             sub: Option<&str>,
                             initial: bool,
                             setter: Box<dyn Fn(&mut crate::config::DockConfig, bool)>| {
        let sw = gtk4::Switch::new();
        sw.set_active(initial);
        sw.set_valign(gtk4::Align::Center);
        let st = state.clone();
        let cb = on_changed.clone();
        sw.connect_state_set(move |_, active| {
            setter(&mut st.borrow_mut().config.dock, active);
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
        widgets_card.append(&row_with_widget(title, sub, &sw));
    };

    add_widget_toggle(
        "Workspace Switcher",
        Some("Clickable & scrollable workspace pills (Hyprland, Sway, Niri, KDE Wayland, X11)"),
        cfg.show_workspaces,
        Box::new(|d, v| d.show_workspaces = v),
    );
    add_widget_toggle(
        "Sound Bar (MPRIS + Equalizer)",
        Some("Shows album art, live audio visualizer bars, controls & scroll-to-volume"),
        cfg.show_media,
        Box::new(|d, v| d.show_media = v),
    );
    add_widget_toggle(
        "Battery & System Monitor",
        Some("Compact battery status (laptops) + live CPU & RAM bars"),
        cfg.show_sysinfo,
        Box::new(|d, v| d.show_sysinfo = v),
    );
    add_widget_toggle(
        "Show Desktop Button",
        Some("One-click button to reveal an empty desktop / workspace"),
        cfg.show_desktop_btn,
        Box::new(|d, v| d.show_desktop_btn = v),
    );
    content.append(&widgets_card);

    let clock_card = card();
    let cl_title = gtk4::Label::new(Some("Clock & Date Widget"));
    cl_title.set_halign(gtk4::Align::Start);
    cl_title.add_css_class("settings-section-title");
    clock_card.append(&cl_title);

    let add_clock_toggle = |title: &str,
                            sub: Option<&str>,
                            initial: bool,
                            setter: Box<dyn Fn(&mut crate::config::DockConfig, bool)>| {
        let sw = gtk4::Switch::new();
        sw.set_active(initial);
        sw.set_valign(gtk4::Align::Center);
        let st = state.clone();
        let cb = on_changed.clone();
        sw.connect_state_set(move |_, active| {
            setter(&mut st.borrow_mut().config.dock, active);
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
        clock_card.append(&row_with_widget(title, sub, &sw));
    };

    add_clock_toggle(
        "Show Clock on Dock",
        Some("Embedded inside the dock tray"),
        cfg.show_clock,
        Box::new(|d, v| d.show_clock = v),
    );
    add_clock_toggle(
        "24-Hour Format",
        Some("Off for 12-hour AM/PM"),
        cfg.clock_24h,
        Box::new(|d, v| d.clock_24h = v),
    );
    add_clock_toggle(
        "Show Seconds",
        None,
        cfg.clock_show_seconds,
        Box::new(|d, v| d.clock_show_seconds = v),
    );
    add_clock_toggle(
        "Show Date",
        Some("Compact date line underneath the time"),
        cfg.clock_show_date,
        Box::new(|d, v| d.clock_show_date = v),
    );

    let date_fmt_dd = gtk4::DropDown::from_strings(&[
        "DD/MM/YYYY (25/11/2026)",
        "Day, Mon DD (Wed, Nov 25)",
        "Mon DD, YYYY (Nov 25, 2026)",
        "ISO (2026-11-25)",
    ]);
    date_fmt_dd.set_selected(match cfg.clock_date_format {
        ClockDateFormat::DmySlash => 0,
        ClockDateFormat::ShortDayMonth => 1,
        ClockDateFormat::MonthDayYear => 2,
        ClockDateFormat::Iso => 3,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        date_fmt_dd.connect_selected_notify(move |dd| {
            st.borrow_mut().config.dock.clock_date_format = match dd.selected() {
                1 => ClockDateFormat::ShortDayMonth,
                2 => ClockDateFormat::MonthDayYear,
                3 => ClockDateFormat::Iso,
                _ => ClockDateFormat::DmySlash,
            };
            notify_change(&st, &cb);
        });
    }
    clock_card.append(&row_with_widget("Date Format", None, &date_fmt_dd));
    content.append(&clock_card);

    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}

fn build_appearance_tab(
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "Appearance",
        "Fine-tune glass tint, transparency, corner radius, and blur rules",
    );

    let cfg = state.borrow().config.appearance.clone();

    let style_card = card();
    let theme_dd = gtk4::DropDown::from_strings(&["Auto", "Light", "Dark"]);
    theme_dd.set_selected(match cfg.theme {
        ThemeMode::Auto => 0,
        ThemeMode::Light => 1,
        ThemeMode::Dark => 2,
    });
    {
        let st = state.clone();
        let cb = on_changed.clone();
        theme_dd.connect_selected_notify(move |dd| {
            st.borrow_mut().config.appearance.theme = match dd.selected() {
                1 => ThemeMode::Light,
                2 => ThemeMode::Dark,
                _ => ThemeMode::Auto,
            };
            notify_change(&st, &cb);
        });
    }
    style_card.append(&row_with_widget("Theme", Some("Glass tint style"), &theme_dd));

    let opacity_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 0.02);
    opacity_scale.set_value(cfg.opacity);
    opacity_scale.set_size_request(210, -1);
    opacity_scale.set_draw_value(true);
    opacity_scale.set_digits(2);
    opacity_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        opacity_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.appearance.opacity = s.value();
            notify_change(&st, &cb);
        });
    }
    style_card.append(&row_with_widget("Glass Opacity", None, &opacity_scale));

    let dyn_op_sw = gtk4::Switch::new();
    dyn_op_sw.set_active(cfg.dynamic_opacity);
    dyn_op_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        dyn_op_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.appearance.dynamic_opacity = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    style_card.append(&row_with_widget(
        "Dynamic Opacity",
        Some("Clearer glass on empty desktop, more solid when windows are active"),
        &dyn_op_sw,
    ));

    let pywal_sw = gtk4::Switch::new();
    pywal_sw.set_active(cfg.pywal_theme);
    pywal_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        pywal_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.appearance.pywal_theme = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    style_card.append(&row_with_widget(
        "Auto-Theme from Wallpaper (pywal)",
        Some("Reads ~/.cache/wal/colors.json for dock tint and accent"),
        &pywal_sw,
    ));

    let radius_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 32.0, 1.0);
    radius_scale.set_value(cfg.corner_radius as f64);
    radius_scale.set_size_request(210, -1);
    radius_scale.set_draw_value(true);
    radius_scale.set_digits(0);
    radius_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        radius_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.appearance.corner_radius = s.value().round() as u32;
            notify_change(&st, &cb);
        });
    }
    style_card.append(&row_with_widget("Corner Radius (px)", None, &radius_scale));

    let margin_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 24.0, 1.0);
    margin_scale.set_value(cfg.edge_margin as f64);
    margin_scale.set_size_request(210, -1);
    margin_scale.set_draw_value(true);
    margin_scale.set_digits(0);
    margin_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        margin_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.appearance.edge_margin = s.value().round() as u32;
            notify_change(&st, &cb);
        });
    }
    style_card.append(&row_with_widget("Edge Margin (px)", None, &margin_scale));

    let sep_sw = gtk4::Switch::new();
    sep_sw.set_active(cfg.show_separators);
    sep_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        sep_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.appearance.show_separators = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    style_card.append(&row_with_widget(
        "Show Separators",
        Some("Divider lines between pinned, running, and Trash"),
        &sep_sw,
    ));

    let halo_sw = gtk4::Switch::new();
    halo_sw.set_active(cfg.hover_highlight);
    halo_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        halo_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.appearance.hover_highlight = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    style_card.append(&row_with_widget(
        "Icon Hover Highlight",
        Some("Soft rounded glow behind hovered icon"),
        &halo_sw,
    ));

    let shadow_sw = gtk4::Switch::new();
    shadow_sw.set_active(cfg.icon_shadow);
    shadow_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        shadow_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.appearance.icon_shadow = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    style_card.append(&row_with_widget(
        "Icon Drop Shadow",
        Some("Soft depth shadow beneath dock icons"),
        &shadow_sw,
    ));

    let sheen_sw = gtk4::Switch::new();
    sheen_sw.set_active(cfg.cursor_sheen);
    sheen_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        sheen_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.appearance.cursor_sheen = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    style_card.append(&row_with_widget(
        "Cursor Glass Sheen",
        Some("Optical glass highlight gliding along the dock with the pointer"),
        &sheen_sw,
    ));

    let aura_sw = gtk4::Switch::new();
    aura_sw.set_active(cfg.adaptive_aura);
    aura_sw.set_valign(gtk4::Align::Center);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        aura_sw.connect_state_set(move |_, active| {
            st.borrow_mut().config.appearance.adaptive_aura = active;
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
    }
    style_card.append(&row_with_widget(
        "Adaptive Icon Aura",
        Some("Extracts each app icon's dominant color for hover glow & indicator tint"),
        &aura_sw,
    ));
    content.append(&style_card);

    let fx_card = card();
    let fx_title = gtk4::Label::new(Some("Custom Colors & Effects"));
    fx_title.set_halign(gtk4::Align::Start);
    fx_title.add_css_class("settings-section-title");
    fx_card.append(&fx_title);

    let tint_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let tint_dialog = gtk4::ColorDialog::new();
    let tint_btn = gtk4::ColorDialogButton::new(Some(tint_dialog));
    if let Some((r, g, b)) = cfg.custom_tint.as_deref().and_then(parse_hex_rgb) {
        tint_btn.set_rgba(&gdk::RGBA::new(r as f32, g as f32, b as f32, 1.0));
    } else {
        tint_btn.set_rgba(&gdk::RGBA::new(0.11, 0.11, 0.14, 1.0));
    }
    {
        let st = state.clone();
        let cb = on_changed.clone();
        tint_btn.connect_rgba_notify(move |b| {
            let c = b.rgba();
            let hex = format!(
                "#{:02x}{:02x}{:02x}",
                (c.red() * 255.0).round() as u8,
                (c.green() * 255.0).round() as u8,
                (c.blue() * 255.0).round() as u8
            );
            st.borrow_mut().config.appearance.custom_tint = Some(hex);
            notify_change(&st, &cb);
        });
    }
    let tint_reset = gtk4::Button::with_label("Auto");
    {
        let st = state.clone();
        let cb = on_changed.clone();
        tint_reset.connect_clicked(move |_| {
            st.borrow_mut().config.appearance.custom_tint = None;
            notify_change(&st, &cb);
        });
    }
    tint_box.append(&tint_btn);
    tint_box.append(&tint_reset);
    fx_card.append(&row_with_widget(
        "Custom Glass Tint",
        Some("Override dock glass background color"),
        &tint_box,
    ));

    let bw_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 3.5, 0.25);
    bw_scale.set_value(cfg.border_width);
    bw_scale.set_size_request(210, -1);
    bw_scale.set_draw_value(true);
    bw_scale.set_digits(2);
    bw_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        bw_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.appearance.border_width = s.value();
            notify_change(&st, &cb);
        });
    }
    fx_card.append(&row_with_widget("Border Width (px)", None, &bw_scale));

    let neon_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.2, 3.5, 0.1);
    neon_scale.set_value(cfg.neon_speed);
    neon_scale.set_size_request(210, -1);
    neon_scale.set_draw_value(true);
    neon_scale.set_digits(1);
    neon_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        neon_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.appearance.neon_speed = s.value();
            notify_change(&st, &cb);
        });
    }
    fx_card.append(&row_with_widget(
        "CyberNeon Speed",
        Some("RGB border rotation speed for Cyber Neon style"),
        &neon_scale,
    ));
    content.append(&fx_card);

    let blur_card = card();
    let hypr_lua = is_hyprland_lua_active();
    let hypr_file_name = if hypr_lua { "hyprland.lua" } else { "hyprland.conf" };
    let blur_sw = gtk4::Switch::new();
    blur_sw.set_active(is_hyprland_blur_enabled());
    blur_sw.set_valign(gtk4::Align::Center);
    let blur_status = gtk4::Label::new(Some(&if is_hyprland_blur_enabled() {
        format!("Active in ~/.config/hypr/{hypr_file_name}")
    } else {
        "Toggle ON to automatically apply & reload Hyprland blur".to_string()
    }));
    blur_status.set_halign(gtk4::Align::Start);
    blur_status.add_css_class("settings-muted");

    {
        let status_c = blur_status.clone();
        blur_sw.connect_state_set(move |_, active| {
            let file_name = if is_hyprland_lua_active() {
                "hyprland.lua"
            } else {
                "hyprland.conf"
            };
            match set_hyprland_blur(active) {
                Ok(()) => status_c.set_text(&if active {
                    format!("Applied & reloaded Hyprland blur ({file_name})!")
                } else {
                    format!("Removed VeloDock blur rules from {file_name}")
                }),
                Err(err) => status_c.set_text(&format!("Manual setup needed: {err}")),
            }
            gtk4::glib::Propagation::Proceed
        });
    }
    let blur_sub = format!("Automatically manages blur rules in ~/.config/hypr/{hypr_file_name}");
    blur_card.append(&row_with_widget(
        "Hyprland Frosted Glass Blur (Auto)",
        Some(&blur_sub),
        &blur_sw,
    ));
    blur_card.append(&blur_status);

    let snippet = if hypr_lua {
        HYPRLAND_LUA_BLUR_BODY
    } else {
        HYPRLAND_CONF_BLUR_BODY
    };
    let code_lbl = gtk4::Label::new(Some(snippet));
    code_lbl.set_halign(gtk4::Align::Fill);
    code_lbl.set_xalign(0.0);
    code_lbl.set_selectable(true);
    code_lbl.add_css_class("settings-code");
    blur_card.append(&code_lbl);

    let copy_btn = gtk4::Button::with_label("Copy Hyprland Rules");
    copy_btn.set_halign(gtk4::Align::Start);
    copy_btn.connect_clicked(move |btn| {
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(snippet);
            btn.set_label("Copied!");
        }
    });
    blur_card.append(&copy_btn);
    content.append(&blur_card);

    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}

fn build_launchpad_tab(
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
    on_toggle_launchpad: Rc<dyn Fn()>,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "Launchpad",
        "Customize the fullscreen app launcher grid, paged view, and frosted backdrop",
    );

    let cfg = state.borrow().config.launchpad.clone();

    let grid_card = card();
    let preview_btn = gtk4::Button::with_label("Preview Launchpad");
    preview_btn.add_css_class("suggested-action");
    {
        let toggle_c = on_toggle_launchpad.clone();
        preview_btn.connect_clicked(move |_| toggle_c());
    }
    grid_card.append(&row_with_widget(
        "Live Preview",
        Some("Open Launchpad now (press ESC or click backdrop to close)"),
        &preview_btn,
    ));

    let cols_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 4.0, 10.0, 1.0);
    cols_scale.set_value(cfg.columns as f64);
    cols_scale.set_size_request(210, -1);
    cols_scale.set_draw_value(true);
    cols_scale.set_digits(0);
    cols_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        cols_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.launchpad.columns = s.value().round() as u32;
            notify_change(&st, &cb);
        });
    }
    grid_card.append(&row_with_widget("Columns", Some("Icons per row"), &cols_scale));

    let rows_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 3.0, 7.0, 1.0);
    rows_scale.set_value(cfg.rows as f64);
    rows_scale.set_size_request(210, -1);
    rows_scale.set_draw_value(true);
    rows_scale.set_digits(0);
    rows_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        rows_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.launchpad.rows = s.value().round() as u32;
            notify_change(&st, &cb);
        });
    }
    grid_card.append(&row_with_widget(
        "Rows per Page",
        Some("Used when Paged Grid Mode is enabled"),
        &rows_scale,
    ));

    let icon_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 44.0, 96.0, 2.0);
    icon_scale.set_value(cfg.icon_size as f64);
    icon_scale.set_size_request(210, -1);
    icon_scale.set_draw_value(true);
    icon_scale.set_digits(0);
    icon_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        icon_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.launchpad.icon_size = s.value().round() as u32;
            notify_change(&st, &cb);
        });
    }
    grid_card.append(&row_with_widget("Icon Size (px)", None, &icon_scale));

    let hover_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 1.0, 1.24, 0.02);
    hover_scale.set_value(cfg.tile_hover_scale);
    hover_scale.set_size_request(210, -1);
    hover_scale.set_draw_value(true);
    hover_scale.set_digits(2);
    hover_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        hover_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.launchpad.tile_hover_scale = s.value();
            notify_change(&st, &cb);
        });
    }
    grid_card.append(&row_with_widget(
        "Tile Hover Zoom",
        Some("Smooth lift scale when hovering over an app"),
        &hover_scale,
    ));
    content.append(&grid_card);

    let style_card = card();
    let op_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.15, 0.96, 0.02);
    op_scale.set_value(cfg.backdrop_opacity);
    op_scale.set_size_request(210, -1);
    op_scale.set_draw_value(true);
    op_scale.set_digits(2);
    op_scale.set_value_pos(gtk4::PositionType::Right);
    {
        let st = state.clone();
        let cb = on_changed.clone();
        op_scale.connect_value_changed(move |s| {
            st.borrow_mut().config.launchpad.backdrop_opacity = s.value();
            notify_change(&st, &cb);
        });
    }
    style_card.append(&row_with_widget(
        "Backdrop Opacity",
        Some("Lower = more wallpaper shows through (great with blur)"),
        &op_scale,
    ));

    let bg_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let bg_dialog = gtk4::ColorDialog::new();
    let bg_btn = gtk4::ColorDialogButton::new(Some(bg_dialog));
    if let Some((r, g, b)) = cfg.custom_bg.as_deref().and_then(parse_hex_rgb) {
        bg_btn.set_rgba(&gdk::RGBA::new(r as f32, g as f32, b as f32, 1.0));
    } else {
        bg_btn.set_rgba(&gdk::RGBA::new(0.055, 0.059, 0.086, 1.0));
    }
    {
        let st = state.clone();
        let cb = on_changed.clone();
        bg_btn.connect_rgba_notify(move |b| {
            let c = b.rgba();
            let hex = format!(
                "#{:02x}{:02x}{:02x}",
                (c.red() * 255.0).round() as u8,
                (c.green() * 255.0).round() as u8,
                (c.blue() * 255.0).round() as u8
            );
            st.borrow_mut().config.launchpad.custom_bg = Some(hex);
            notify_change(&st, &cb);
        });
    }
    let bg_reset = gtk4::Button::with_label("Auto");
    {
        let st = state.clone();
        let cb = on_changed.clone();
        bg_reset.connect_clicked(move |_| {
            st.borrow_mut().config.launchpad.custom_bg = None;
            notify_change(&st, &cb);
        });
    }
    bg_box.append(&bg_btn);
    bg_box.append(&bg_reset);
    style_card.append(&row_with_widget(
        "Backdrop Tint Color",
        Some("Custom color tint for Launchpad overlay"),
        &bg_box,
    ));

    let add_lp_toggle = |title: &str,
                         sub: Option<&str>,
                         initial: bool,
                         setter: Box<dyn Fn(&mut crate::config::LaunchpadConfig, bool)>| {
        let sw = gtk4::Switch::new();
        sw.set_active(initial);
        sw.set_valign(gtk4::Align::Center);
        let st = state.clone();
        let cb = on_changed.clone();
        sw.connect_state_set(move |_, active| {
            setter(&mut st.borrow_mut().config.launchpad, active);
            notify_change(&st, &cb);
            gtk4::glib::Propagation::Proceed
        });
        style_card.append(&row_with_widget(title, sub, &sw));
    };

    add_lp_toggle(
        "Paged Grid Mode",
        Some("Split apps into pages with ● ○ ○ dots (scroll or ← / → to flip)"),
        cfg.paged_mode,
        Box::new(|l, v| l.paged_mode = v),
    );
    add_lp_toggle(
        "Show Category Filter Pills",
        Some("Quick filter bar (Internet, Dev, Media, Games, System)"),
        cfg.show_categories,
        Box::new(|l, v| l.show_categories = v),
    );
    add_lp_toggle(
        "Show Application Names",
        Some("Display app labels under icons"),
        cfg.show_labels,
        Box::new(|l, v| l.show_labels = v),
    );
    content.append(&style_card);

    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}

fn build_pinned_apps_tab(
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "Pinned Apps",
        "Manage and reorder applications pinned to your dock",
    );

    let pinned_card = card();
    let header_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let title = gtk4::Label::new(Some("Pinned Applications"));
    title.set_hexpand(true);
    title.set_halign(gtk4::Align::Start);
    title.add_css_class("settings-section-title");
    header_row.append(&title);

    let reset_btn = gtk4::Button::with_label("Reset Defaults");
    header_row.append(&reset_btn);
    pinned_card.append(&header_row);

    let pinned_list = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    pinned_card.append(&pinned_list);
    content.append(&pinned_card);

    let add_card = card();
    let add_title = gtk4::Label::new(Some("Add Application"));
    add_title.set_halign(gtk4::Align::Start);
    add_title.add_css_class("settings-section-title");
    add_card.append(&add_title);

    let search = gtk4::SearchEntry::new();
    search.set_placeholder_text(Some("Search installed apps to pin..."));
    add_card.append(&search);

    let results_list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    add_card.append(&results_list);
    content.append(&add_card);

    let stacks_card = card();
    let st_title = gtk4::Label::new(Some("Folder Stacks (Next to Trash)"));
    st_title.set_halign(gtk4::Align::Start);
    st_title.add_css_class("settings-section-title");
    stacks_card.append(&st_title);

    let st_desc = gtk4::Label::new(Some(
        "Pin folders to open recent files in a Stack popup:",
    ));
    st_desc.set_halign(gtk4::Align::Start);
    st_desc.add_css_class("settings-muted");
    stacks_card.append(&st_desc);

    let stacks_list = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    stacks_card.append(&stacks_list);

    let rebuild_stacks: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let rebuild_stacks_ref = rebuild_stacks.clone();
    {
        let state_c = state.clone();
        let on_changed_c = on_changed.clone();
        let list_c = stacks_list.clone();
        let f_rc: Rc<dyn Fn()> = Rc::new(move || {
            while let Some(child) = list_c.first_child() {
                list_c.remove(&child);
            }
            let folders = state_c.borrow().config.apps.pinned_folders.clone();
            for path in folders {
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let lbl = gtk4::Label::new(Some(&path));
                lbl.set_hexpand(true);
                lbl.set_halign(gtk4::Align::Start);
                lbl.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
                row.append(&lbl);

                let rm = gtk4::Button::with_label("Remove");
                let st = state_c.clone();
                let cb = on_changed_c.clone();
                let rb = rebuild_stacks_ref.clone();
                let p_c = path.clone();
                rm.connect_clicked(move |_| {
                    st.borrow_mut().config.apps.pinned_folders.retain(|x| x != &p_c);
                    notify_change(&st, &cb);
                    if let Some(f) = rb.borrow().as_ref() {
                        f();
                    }
                });
                row.append(&rm);
                list_c.append(&row);
            }
        });
        *rebuild_stacks.borrow_mut() = Some(f_rc.clone());
        f_rc();
    }

    let quick_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let quick_dirs: [(&str, Option<std::path::PathBuf>); 4] = [
        ("+ Downloads", dirs::download_dir()),
        ("+ Documents", dirs::document_dir()),
        ("+ Pictures", dirs::picture_dir()),
        ("+ Music", dirs::audio_dir()),
    ];
    for (lbl, dir_opt) in quick_dirs {
        if let Some(dir) = dir_opt {
            let btn = gtk4::Button::with_label(lbl);
            let path_str = dir.to_string_lossy().into_owned();
            let st = state.clone();
            let cb = on_changed.clone();
            let rb = rebuild_stacks.clone();
            btn.connect_clicked(move |_| {
                {
                    let mut s = st.borrow_mut();
                    if !s.config.apps.pinned_folders.contains(&path_str) {
                        s.config.apps.pinned_folders.push(path_str.clone());
                    }
                }
                notify_change(&st, &cb);
                if let Some(f) = rb.borrow().as_ref() {
                    f();
                }
            });
            quick_row.append(&btn);
        }
    }
    stacks_card.append(&quick_row);
    content.append(&stacks_card);

    let rebuild_pinned: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let rebuild_pinned_ref = rebuild_pinned.clone();
    {
        let state_c = state.clone();
        let on_changed_c = on_changed.clone();
        let list_c = pinned_list.clone();
        let fn_rc: Rc<dyn Fn()> = Rc::new(move || {
            while let Some(child) = list_c.first_child() {
                list_c.remove(&child);
            }
            let pinned: Vec<(String, String)> = {
                let st = state_c.borrow();
                st.config
                    .apps
                    .pinned
                    .iter()
                    .map(|id| {
                        let name = st
                            .desktop_index
                            .get(id)
                            .map(|e| e.name.clone())
                            .unwrap_or_else(|| id.clone());
                        (id.clone(), name)
                    })
                    .collect()
            };
            let len = pinned.len();
            for (idx, (id, name)) in pinned.into_iter().enumerate() {
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                let lbl = gtk4::Label::new(Some(&name));
                lbl.set_hexpand(true);
                lbl.set_halign(gtk4::Align::Start);
                row.append(&lbl);

                let up = gtk4::Button::with_label("↑");
                up.set_sensitive(idx > 0);
                {
                    let st = state_c.clone();
                    let cb = on_changed_c.clone();
                    let rb = rebuild_pinned_ref.clone();
                    up.connect_clicked(move |_| {
                        st.borrow_mut().config.apps.pinned.swap(idx, idx - 1);
                        notify_change(&st, &cb);
                        if let Some(f) = rb.borrow().as_ref() {
                            f();
                        }
                    });
                }
                row.append(&up);

                let down = gtk4::Button::with_label("↓");
                down.set_sensitive(idx + 1 < len);
                {
                    let st = state_c.clone();
                    let cb = on_changed_c.clone();
                    let rb = rebuild_pinned_ref.clone();
                    down.connect_clicked(move |_| {
                        st.borrow_mut().config.apps.pinned.swap(idx, idx + 1);
                        notify_change(&st, &cb);
                        if let Some(f) = rb.borrow().as_ref() {
                            f();
                        }
                    });
                }
                row.append(&down);

                let rm = gtk4::Button::with_label("Remove");
                {
                    let st = state_c.clone();
                    let cb = on_changed_c.clone();
                    let rb = rebuild_pinned_ref.clone();
                    let id_c = id.clone();
                    rm.connect_clicked(move |_| {
                        st.borrow_mut().config.apps.pinned.retain(|p| p != &id_c);
                        notify_change(&st, &cb);
                        if let Some(f) = rb.borrow().as_ref() {
                            f();
                        }
                    });
                }
                row.append(&rm);
                list_c.append(&row);
            }
        });
        *rebuild_pinned.borrow_mut() = Some(fn_rc.clone());
        fn_rc();
    }

    {
        let st = state.clone();
        let cb = on_changed.clone();
        let rb = rebuild_pinned.clone();
        reset_btn.connect_clicked(move |_| {
            let defaults = {
                let s = st.borrow();
                detect_default_pinned(&s.desktop_index.all_ids())
            };
            st.borrow_mut().config.apps.pinned = defaults;
            notify_change(&st, &cb);
            if let Some(f) = rb.borrow().as_ref() {
                f();
            }
        });
    }

    let populate_search = {
        let state_c = state.clone();
        let cb_c = on_changed.clone();
        let rb_c = rebuild_pinned.clone();
        let res_c = results_list.clone();
        Rc::new(move |query: &str| {
            while let Some(child) = res_c.first_child() {
                res_c.remove(&child);
            }
            let q = query.trim().to_lowercase();
            let matches: Vec<(String, String)> = {
                let st = state_c.borrow();
                st.desktop_index
                    .visible_sorted()
                    .into_iter()
                    .filter(|e| {
                        !st.config.apps.pinned.contains(&e.id)
                            && (q.is_empty() || e.name.to_lowercase().contains(&q))
                    })
                    .take(12)
                    .map(|e| (e.id.clone(), e.name.clone()))
                    .collect()
            };
            for (id, name) in matches {
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let lbl = gtk4::Label::new(Some(&name));
                lbl.set_hexpand(true);
                lbl.set_halign(gtk4::Align::Start);
                row.append(&lbl);

                let add = gtk4::Button::with_label("+ Pin");
                let st = state_c.clone();
                let cb = cb_c.clone();
                let rb = rb_c.clone();
                let row_c = row.clone();
                let res_cc = res_c.clone();
                add.connect_clicked(move |_| {
                    {
                        let mut s = st.borrow_mut();
                        if !s.config.apps.pinned.contains(&id) {
                            s.config.apps.pinned.push(id.clone());
                        }
                    }
                    notify_change(&st, &cb);
                    if let Some(f) = rb.borrow().as_ref() {
                        f();
                    }
                    res_cc.remove(&row_c);
                });
                row.append(&add);
                res_c.append(&row);
            }
        })
    };

    populate_search("");
    {
        let pop = populate_search.clone();
        search.connect_search_changed(move |entry| {
            pop(&entry.text());
        });
    }

    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}

fn build_general_tab(
    app: gtk4::Application,
    state: Rc<RefCell<AppState>>,
    on_changed: Rc<dyn Fn()>,
    parent_win: gtk4::Window,
) -> gtk4::ScrolledWindow {
    let content = page_container(
        "General & Autostart",
        "Control startup behavior, backup/restore config, and application actions",
    );

    let startup_card = card();
    let autostart_sw = gtk4::Switch::new();
    autostart_sw.set_active(is_autostart_enabled());
    autostart_sw.set_valign(gtk4::Align::Center);
    autostart_sw.connect_state_set(move |_, active| {
        if let Err(err) = set_autostart(active) {
            eprintln!("velodock: failed to update autostart: {err}");
        }
        gtk4::glib::Propagation::Proceed
    });
    startup_card.append(&row_with_widget(
        "Start automatically on login (Dock only)",
        Some("Auto-configures XDG Autostart + Hyprland/Sway config (runs `velodock --dock-only`)"),
        &autostart_sw,
    ));
    content.append(&startup_card);

    let hypr_card = card();
    let h_title = gtk4::Label::new(Some("Hyprland / Sway Autostart"));
    h_title.set_halign(gtk4::Align::Start);
    h_title.add_css_class("settings-section-title");
    hypr_card.append(&h_title);

    let h_desc = gtk4::Label::new(Some(
        "For tiling compositors (Hyprland / Sway), add this line to start only the Dock silently:",
    ));
    h_desc.set_halign(gtk4::Align::Start);
    h_desc.set_wrap(true);
    h_desc.add_css_class("settings-muted");
    hypr_card.append(&h_desc);

    let exec_snippet = "exec-once = velodock --dock-only";
    let code = gtk4::Label::new(Some(exec_snippet));
    code.set_halign(gtk4::Align::Fill);
    code.set_xalign(0.0);
    code.set_selectable(true);
    code.add_css_class("settings-code");
    hypr_card.append(&code);

    let copy_exec = gtk4::Button::with_label("Copy exec-once");
    copy_exec.set_halign(gtk4::Align::Start);
    copy_exec.connect_clicked(move |b| {
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(exec_snippet);
            b.set_label("Copied!");
        }
    });
    hypr_card.append(&copy_exec);
    content.append(&hypr_card);

    let io_card = card();
    let io_title = gtk4::Label::new(Some("Backup & Share Configuration"));
    io_title.set_halign(gtk4::Align::Start);
    io_title.add_css_class("settings-section-title");
    io_card.append(&io_title);

    let io_btns = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let export_btn = gtk4::Button::with_label("Export Config...");
    {
        let st = state.clone();
        let win = parent_win.clone();
        export_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title("Export VeloDock Config")
                .initial_name("velodock-config.toml")
                .build();
            let st_c = st.clone();
            dialog.save(Some(&win), gtk4::gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let _ = st_c.borrow().config.save_to_path(&path);
                    }
                }
            });
        });
    }
    let import_btn = gtk4::Button::with_label("Import Config...");
    {
        let st = state.clone();
        let cb = on_changed.clone();
        let win = parent_win.clone();
        import_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title("Import VeloDock Config")
                .build();
            let st_c = st.clone();
            let cb_c = cb.clone();
            dialog.open(Some(&win), gtk4::gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let (loaded, _created) = crate::config::Config::load_from_path(&path);
                        st_c.borrow_mut().config = loaded;
                        notify_change(&st_c, &cb_c);
                    }
                }
            });
        });
    }
    io_btns.append(&export_btn);
    io_btns.append(&import_btn);
    io_card.append(&row_with_widget(
        "Export / Import TOML",
        Some("Save or restore your entire VeloDock setup in 1 click"),
        &io_btns,
    ));
    content.append(&io_card);

    let actions_card = card();
    let quit_btn = gtk4::Button::with_label("Quit VeloDock");
    quit_btn.add_css_class("destructive-action");
    let app_c = app.clone();
    quit_btn.connect_clicked(move |_| {
        app_c.quit();
    });
    actions_card.append(&row_with_widget(
        "Quit Application",
        Some("Closes both the Dock and Settings window"),
        &quit_btn,
    ));
    content.append(&actions_card);

    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build()
}
