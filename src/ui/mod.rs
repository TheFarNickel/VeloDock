pub mod context_menu;
pub mod dock_window;
pub mod launchpad;
pub mod settings_window;

use crate::config::{Config, ThemeMode};
use crate::desktop::DesktopIndex;
use crate::platform::{WindowInfo, WindowTracker};
use crate::trash::TrashDir;
use gtk4::gdk;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

pub struct AppState {
    pub config: Config,
    pub config_path: PathBuf,
    pub desktop_index: DesktopIndex,
    pub trash: TrashDir,
    pub windows: Vec<WindowInfo>,
    pub tracker: Box<dyn WindowTracker>,
    save_timer: Option<glib::SourceId>,
}

impl AppState {
    pub fn new(
        config: Config,
        config_path: PathBuf,
        desktop_index: DesktopIndex,
        trash: TrashDir,
        tracker: Box<dyn WindowTracker>,
    ) -> Self {
        Self {
            config,
            config_path,
            desktop_index,
            trash,
            windows: Vec::new(),
            tracker,
            save_timer: None,
        }
    }

    pub fn schedule_save(state: &Rc<RefCell<Self>>) {
        let mut st = state.borrow_mut();
        if let Some(id) = st.save_timer.take() {
            id.remove();
        }
        let state_weak = Rc::downgrade(state);
        let id = glib::timeout_add_local_once(Duration::from_millis(250), move || {
            if let Some(state_rc) = state_weak.upgrade() {
                let mut s = state_rc.borrow_mut();
                s.save_timer = None;
                if let Err(err) = s.config.save_to_path(&s.config_path) {
                    eprintln!("archdock: failed to save config: {err}");
                }
            }
        });
        st.save_timer = Some(id);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GlassPalette {
    pub bg: (f64, f64, f64, f64),
    pub border_outer: (f64, f64, f64, f64),
    pub border_inner: (f64, f64, f64, f64),
    pub separator: (f64, f64, f64, f64),
    pub dot: (f64, f64, f64, f64),
    pub tooltip_bg: (f64, f64, f64, f64),
    pub tooltip_border: (f64, f64, f64, f64),
    pub tooltip_fg: (f64, f64, f64, f64),
}

pub fn resolve_dark_mode(mode: ThemeMode) -> bool {
    match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::Auto => gtk4::Settings::default()
            .map(|s| s.is_gtk_application_prefer_dark_theme())
            .unwrap_or(true),
    }
}

pub fn glass_palette(
    mode: ThemeMode,
    opacity: f64,
    custom_tint: Option<&str>,
    custom_indicator: Option<&str>,
) -> GlassPalette {
    let dark = resolve_dark_mode(mode);
    let a = opacity.clamp(0.0, 1.0);
    let mut pal = if dark {
        GlassPalette {
            bg: (0.11, 0.11, 0.14, a),
            border_outer: (0.0, 0.0, 0.0, 0.45),
            border_inner: (1.0, 1.0, 1.0, 0.18),
            separator: (1.0, 1.0, 1.0, 0.24),
            dot: (0.95, 0.96, 1.0, 0.92),
            tooltip_bg: (0.14, 0.14, 0.18, 0.92),
            tooltip_border: (1.0, 1.0, 1.0, 0.20),
            tooltip_fg: (0.98, 0.98, 1.0, 1.0),
        }
    } else {
        GlassPalette {
            bg: (0.95, 0.95, 0.97, a),
            border_outer: (0.0, 0.0, 0.0, 0.16),
            border_inner: (1.0, 1.0, 1.0, 0.65),
            separator: (0.15, 0.15, 0.20, 0.22),
            dot: (0.18, 0.18, 0.22, 0.85),
            tooltip_bg: (0.96, 0.96, 0.98, 0.94),
            tooltip_border: (0.0, 0.0, 0.0, 0.14),
            tooltip_fg: (0.10, 0.10, 0.12, 1.0),
        }
    };
    if let Some((r, g, b)) = custom_tint.and_then(crate::config::parse_hex_rgb) {
        pal.bg = (r, g, b, a);
    }
    if let Some((r, g, b)) = custom_indicator.and_then(crate::config::parse_hex_rgb) {
        pal.dot = (r, g, b, 0.96);
    }
    pal
}

#[derive(Default)]
pub struct IconCache {
    cache: HashMap<(String, i32), Option<Pixbuf>>,
    colors: HashMap<String, Option<(f64, f64, f64)>>,
}

impl IconCache {
    pub fn get(&mut self, icon_name: &str, size: i32) -> Option<Pixbuf> {
        let size = size.clamp(16, 256);
        let key = (icon_name.to_string(), size);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let loaded = load_icon_pixbuf(icon_name, size)
            .or_else(|| load_icon_pixbuf("application-x-executable", size));
        self.cache.insert(key, loaded.clone());
        loaded
    }

    pub fn dominant_color(&mut self, icon_name: &str, size: i32) -> Option<(f64, f64, f64)> {
        if let Some(cached) = self.colors.get(icon_name) {
            return *cached;
        }
        let color = self.get(icon_name, size).and_then(|pb| extract_dominant_rgb(&pb));
        self.colors.insert(icon_name.to_string(), color);
        color
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.colors.clear();
    }
}

fn extract_dominant_rgb(pb: &Pixbuf) -> Option<(f64, f64, f64)> {
    let w = pb.width() as usize;
    let h = pb.height() as usize;
    let stride = pb.rowstride() as usize;
    let n_ch = pb.n_channels() as usize;
    if w == 0 || h == 0 || n_ch < 3 {
        return None;
    }
    let pixels = unsafe { pb.pixels() };
    let step_x = (w / 18).max(1);
    let step_y = (h / 18).max(1);
    let mut sum_r = 0.0_f64;
    let mut sum_g = 0.0_f64;
    let mut sum_b = 0.0_f64;
    let mut sum_w = 0.0_f64;

    let mut y = 0;
    while y < h {
        let row = y * stride;
        let mut x = 0;
        while x < w {
            let idx = row + x * n_ch;
            if idx + n_ch <= pixels.len() {
                let a = if n_ch >= 4 { pixels[idx + 3] as f64 / 255.0 } else { 1.0 };
                if a > 0.45 {
                    let r = pixels[idx] as f64 / 255.0;
                    let g = pixels[idx + 1] as f64 / 255.0;
                    let b = pixels[idx + 2] as f64 / 255.0;
                    let max_c = r.max(g).max(b);
                    let min_c = r.min(g).min(b);
                    let sat = if max_c > 0.01 { (max_c - min_c) / max_c } else { 0.0 };
                    if sat > 0.18 && max_c > 0.18 {
                        let weight = (sat * sat + 0.15) * a;
                        sum_r += r * weight;
                        sum_g += g * weight;
                        sum_b += b * weight;
                        sum_w += weight;
                    }
                }
            }
            x += step_x;
        }
        y += step_y;
    }
    if sum_w <= 0.01 {
        return None;
    }
    let mut r = sum_r / sum_w;
    let mut g = sum_g / sum_w;
    let mut b = sum_b / sum_w;
    let peak = r.max(g).max(b).max(0.01);
    if peak < 0.78 {
        let boost = 0.85 / peak;
        r = (r * boost).min(1.0);
        g = (g * boost).min(1.0);
        b = (b * boost).min(1.0);
    }
    Some((r, g, b))
}

fn load_icon_pixbuf(name: &str, size: i32) -> Option<Pixbuf> {
    if name.is_empty() {
        return None;
    }
    if name.eq_ignore_ascii_case("velodock") {
        if let Some(pb) = load_embedded_svg(include_bytes!("../../assets/icons/velodock.svg"), size) {
            return Some(pb);
        }
    }
    if name.eq_ignore_ascii_case("velodock-settings")
        || name.eq_ignore_ascii_case("io.github.velodock.app")
    {
        if let Some(pb) =
            load_embedded_svg(include_bytes!("../../assets/icons/velodock-settings.svg"), size)
        {
            return Some(pb);
        }
    }

    let path = Path::new(name);
    if path.is_absolute() && path.exists() {
        if let Ok(pb) = Pixbuf::from_file_at_scale(path, size, size, true) {
            return Some(pb);
        }
    }

    let display = gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);
    let paintable = theme.lookup_icon(
        name,
        &[],
        size,
        1,
        gtk4::TextDirection::None,
        gtk4::IconLookupFlags::empty(),
    );
    if let Some(file) = paintable.file() {
        if let Some(p) = file.path() {
            if let Ok(pb) = Pixbuf::from_file_at_scale(p, size, size, true) {
                return Some(pb);
            }
        }
    }
    None
}

fn load_embedded_svg(bytes: &[u8], size: i32) -> Option<Pixbuf> {
    let loader = gtk4::gdk_pixbuf::PixbufLoader::new();
    loader.set_size(size, size);
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    loader.pixbuf()
}

pub fn install_global_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(
        r#"
window.archdock-window {
    background-color: transparent;
    background-image: none;
    box-shadow: none;
    border: none;
}

popover.archdock-menu > contents {
    background-color: rgba(26, 26, 32, 0.94);
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 13px;
    padding: 6px;
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.52);
}

.archdock-menu-header {
    font-size: 11px;
    font-weight: 700;
    color: rgba(235, 235, 245, 0.50);
    padding: 4px 10px 6px 10px;
}

button.archdock-menu-item {
    background: transparent;
    border: none;
    border-radius: 7px;
    padding: 6px 12px;
    color: #f5f5f7;
    font-size: 13px;
    font-weight: 500;
    transition: background-color 120ms ease;
}

button.archdock-menu-item:hover {
    background-color: #0a84ff;
    color: #ffffff;
}

button.archdock-menu-item.destructive {
    color: #ff6961;
}

button.archdock-menu-item.destructive:hover {
    background-color: #ff453a;
    color: #ffffff;
}

window.archdock-launchpad {
    background-color: transparent;
}

.launchpad-backdrop {
    background-color: rgba(14, 15, 20, 0.84);
    opacity: 0;
    transition: opacity 210ms cubic-bezier(0.16, 1, 0.3, 1);
}

.launchpad-backdrop.open {
    opacity: 1;
}

.launchpad-content {
    opacity: 0;
    transform: scale(1.10);
    transition: opacity 220ms cubic-bezier(0.16, 1, 0.3, 1),
                transform 240ms cubic-bezier(0.16, 1, 0.3, 1);
}

.launchpad-content.open {
    opacity: 1;
    transform: scale(1.0);
}

entry.launchpad-search {
    background-color: rgba(255, 255, 255, 0.13);
    border: 1px solid rgba(255, 255, 255, 0.24);
    border-radius: 999px;
    color: #ffffff;
    font-size: 15px;
    padding: 9px 20px;
    min-width: 340px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.28);
    transition: border-color 150ms ease, background-color 150ms ease;
}

entry.launchpad-search:focus {
    background-color: rgba(255, 255, 255, 0.18);
    border-color: rgba(56, 189, 248, 0.78);
}

button.launchpad-cat-pill {
    background-color: rgba(255, 255, 255, 0.09);
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 999px;
    padding: 5px 14px;
    color: rgba(245, 245, 247, 0.82);
    font-size: 12.5px;
    font-weight: 500;
    transition: background-color 140ms ease, color 140ms ease, border-color 140ms ease;
}

button.launchpad-cat-pill:hover {
    background-color: rgba(255, 255, 255, 0.18);
    color: #ffffff;
}

button.launchpad-cat-pill.active {
    background-color: #0a84ff;
    border-color: #38bdf8;
    color: #ffffff;
    font-weight: 600;
}

button.launchpad-tile {
    background: transparent;
    border: 1px solid transparent;
    border-radius: 18px;
    padding: 12px 8px;
    color: #f5f5f7;
    transition: background-color 150ms ease, border-color 150ms ease, transform 160ms cubic-bezier(0.16, 1, 0.3, 1);
}

button.launchpad-tile:hover,
button.launchpad-tile:focus {
    background-color: rgba(255, 255, 255, 0.13);
    border-color: rgba(255, 255, 255, 0.18);
}

.launchpad-tile-label {
    font-size: 12.5px;
    font-weight: 500;
    color: #ffffff;
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.75);
}

button.launchpad-dot {
    min-width: 9px;
    min-height: 9px;
    padding: 0;
    border-radius: 999px;
    border: none;
    background-color: rgba(255, 255, 255, 0.30);
    transition: background-color 150ms ease, min-width 180ms cubic-bezier(0.16, 1, 0.3, 1);
}

button.launchpad-dot:hover {
    background-color: rgba(255, 255, 255, 0.62);
}

button.launchpad-dot.active {
    min-width: 24px;
    background-color: #ffffff;
}

window.archdock-settings {
    background-color: #1c1c1f;
    color: #f5f5f7;
}

.settings-sidebar {
    background-color: #161619;
    border-right: 1px solid rgba(255, 255, 255, 0.08);
    padding: 14px 10px;
}

.settings-brand-title {
    font-size: 16px;
    font-weight: 800;
    color: #ffffff;
}

.settings-brand-sub {
    font-size: 11px;
    color: rgba(235, 235, 245, 0.50);
}

button.sidebar-nav-btn {
    background: transparent;
    border: none;
    border-radius: 9px;
    padding: 9px 12px;
    color: rgba(245, 245, 247, 0.84);
    font-size: 13px;
    font-weight: 500;
    transition: background-color 140ms ease, color 140ms ease;
}

button.sidebar-nav-btn:hover {
    background-color: rgba(255, 255, 255, 0.07);
    color: #ffffff;
}

button.sidebar-nav-btn.active {
    background-color: #0a84ff;
    color: #ffffff;
    font-weight: 600;
}

.settings-page-title {
    font-size: 20px;
    font-weight: 700;
    color: #ffffff;
    margin-bottom: 4px;
}

.settings-card {
    background-color: #26262a;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 14px 16px;
}

button.style-card {
    background-color: #26262a;
    border: 2px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 10px;
    transition: border-color 150ms ease, background-color 150ms ease;
}

button.style-card:hover {
    background-color: #2e2e33;
    border-color: rgba(255, 255, 255, 0.22);
}

button.style-card.selected {
    background-color: rgba(10, 132, 255, 0.14);
    border-color: #0a84ff;
}

.settings-section-title {
    font-weight: 700;
    font-size: 13px;
    color: rgba(235, 235, 245, 0.72);
    margin-top: 2px;
}

.settings-muted {
    font-size: 12px;
    color: rgba(235, 235, 245, 0.56);
}

.settings-code {
    font-family: monospace;
    font-size: 12px;
    background-color: rgba(0, 0, 0, 0.38);
    border: 1px solid rgba(255, 255, 255, 0.10);
    border-radius: 8px;
    padding: 8px 10px;
    color: #98d8ff;
}
"#,
    );
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
