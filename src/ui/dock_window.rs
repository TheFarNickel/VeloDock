use crate::config::{
    load_pywal_colors, ClockDateFormat, DockPosition, DockStyle, HoverAnimation, IndicatorStyle,
    LeftClickAction, MiddleClickAction, ScrollAction, VisibilityMode,
};
use crate::physics::{
    compute_hover_targets, layout_slots, should_unpin_drag, DockPhysics, HoverTarget, SlotGeometry,
};
use crate::platform::surface::{
    configure_dock_window, reposition_x11_dock, resolve_monitor, update_input_region,
};
use crate::platform::{any_window_overlaps_mode, Rect, WorkspaceInfo};
use crate::ui::context_menu::{show_folder_stack_popover, show_item_menu};
use crate::ui::{glass_palette, AppState, IconCache};
use crate::widgets::{
    mpris_adjust_volume, mpris_command, query_mpris, MediaSnapshot, SysInfoReader, SysInfoSnapshot,
};
use gtk4::cairo;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum DockItemKind {
    Launchpad,
    App {
        desktop_id: String,
        app_id_fallback: Option<String>,
    },
    WindowTile {
        window_id: String,
        title: String,
    },
    Folder {
        path: String,
    },
    Trash,
}

#[derive(Debug, Clone)]
pub struct DockItem {
    pub key: String,
    pub label: String,
    pub icon_name: String,
    pub kind: DockItemKind,
    pub pinned: bool,
    pub window_ids: Vec<String>,
}

struct DragState {
    item_index: usize,
    desktop_id: String,
    start_x: f64,
    start_y: f64,
    current_x: f64,
    current_y: f64,
    active: bool,
}

#[derive(Debug, Clone)]
enum WidgetAction {
    SwitchWorkspace(String),
    MediaPrev(String),
    MediaPlayPause(String),
    MediaNext(String),
    MediaScroll(String),
    ShowDesktop,
}

#[derive(Debug, Clone)]
struct WidgetHitbox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    action: WidgetAction,
}

impl WidgetHitbox {
    fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

struct DockRuntime {
    items: Vec<DockItem>,
    separators: Vec<usize>,
    slots: Vec<SlotGeometry>,
    pill_rect: (f64, f64, f64, f64),
    physics: DockPhysics,
    pointer: Option<(f64, f64)>,
    hovered_index: Option<usize>,
    last_frame: Option<Instant>,
    icon_cache: IconCache,
    drag: Option<DragState>,
    hide_timer: Option<glib::SourceId>,
    last_focused_window: HashMap<String, String>,
    last_input_rect: Option<Rect>,
    last_canvas_size: (i32, i32),
    neon_phase: f64,
    hover_time: f64,
    tick_active: bool,
    workspaces: Vec<WorkspaceInfo>,
    media: Option<MediaSnapshot>,
    sysinfo: SysInfoSnapshot,
    sysinfo_reader: SysInfoReader,
    widget_hitboxes: Vec<WidgetHitbox>,
}

impl Default for DockRuntime {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            separators: Vec::new(),
            slots: Vec::new(),
            pill_rect: (0.0, 0.0, 0.0, 0.0),
            physics: DockPhysics::new(),
            pointer: None,
            hovered_index: None,
            last_frame: None,
            icon_cache: IconCache::default(),
            drag: None,
            hide_timer: None,
            last_focused_window: HashMap::new(),
            last_input_rect: None,
            last_canvas_size: (0, 0),
            neon_phase: 0.0,
            hover_time: 0.0,
            tick_active: false,
            workspaces: Vec::new(),
            media: None,
            sysinfo: SysInfoSnapshot::default(),
            sysinfo_reader: SysInfoReader::default(),
            widget_hitboxes: Vec::new(),
        }
    }
}

pub struct DockWindow {
    window: gtk4::ApplicationWindow,
    drawing_area: gtk4::DrawingArea,
    state: Rc<RefCell<AppState>>,
    runtime: Rc<RefCell<DockRuntime>>,
}

impl DockWindow {
    pub fn new(
        app: &gtk4::Application,
        state: Rc<RefCell<AppState>>,
        on_open_settings: Rc<dyn Fn()>,
        on_toggle_launchpad: Rc<dyn Fn()>,
    ) -> Rc<Self> {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("VeloDock")
            .build();
        window.add_css_class("archdock-window");

        let drawing_area = gtk4::DrawingArea::new();
        drawing_area.set_hexpand(true);
        drawing_area.set_vexpand(true);
        window.set_child(Some(&drawing_area));

        let runtime = Rc::new(RefCell::new(DockRuntime::default()));
        let dock = Rc::new(Self {
            window,
            drawing_area,
            state,
            runtime,
        });

        dock.setup_drawing();
        dock.setup_input(on_open_settings, on_toggle_launchpad);
        dock.setup_tick();
        dock.apply_config();

        let dock_weak = Rc::downgrade(&dock);
        glib::timeout_add_local(Duration::from_millis(1000), move || {
            if let Some(d) = dock_weak.upgrade() {
                d.refresh_widgets();
                d.rebuild_items();
                d.update_canvas_size();
                d.drawing_area.queue_draw();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });

        dock
    }

    fn refresh_widgets(&self) {
        let cfg = self.state.borrow().config.dock.clone();
        let ws = if cfg.show_workspaces {
            self.state.borrow().tracker.workspaces()
        } else {
            Vec::new()
        };
        let media = if cfg.show_media {
            query_mpris()
        } else {
            None
        };
        let mut rt = self.runtime.borrow_mut();
        rt.workspaces = ws;
        rt.media = media;
        if cfg.show_sysinfo {
            rt.sysinfo = rt.sysinfo_reader.sample();
        }
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn window(&self) -> &gtk4::ApplicationWindow {
        &self.window
    }

    pub fn apply_config(&self) {
        let cfg = self.state.borrow().config.clone();
        let icon_sz = cfg.dock.icon_size as i32;
        let effective_margin = match cfg.appearance.style {
            DockStyle::Floating
            | DockStyle::SplitIslands
            | DockStyle::BaselineBar
            | DockStyle::Aurora
            | DockStyle::CyberNeon
            | DockStyle::Capsule
            | DockStyle::Invisible => cfg.appearance.edge_margin as i32,
            DockStyle::Attached | DockStyle::Panel | DockStyle::EdgeRail | DockStyle::Glass3D => 0,
        };
        let pad_pill = match cfg.appearance.style {
            DockStyle::Attached | DockStyle::Capsule => 14,
            DockStyle::Floating
            | DockStyle::Panel
            | DockStyle::EdgeRail
            | DockStyle::Glass3D
            | DockStyle::SplitIslands
            | DockStyle::BaselineBar
            | DockStyle::Aurora
            | DockStyle::CyberNeon
            | DockStyle::Invisible => 12,
        };
        let exclusive_px = icon_sz + pad_pill * 2 + effective_margin;

        configure_dock_window(
            &self.window,
            cfg.dock.position,
            cfg.dock.visibility,
            effective_margin,
            exclusive_px,
            cfg.dock.monitor_name.as_deref(),
        );

        self.refresh_widgets();
        self.rebuild_items();
        self.update_canvas_size();

        {
            let mut rt = self.runtime.borrow_mut();
            rt.icon_cache.clear();
            rt.physics.set_tuning(
                cfg.dock.animation_smoothness,
                cfg.dock.bounce_intensity,
                cfg.dock.launch_animation,
            );
            let target_reveal = match cfg.dock.visibility {
                VisibilityMode::Always => 1.0,
                VisibilityMode::AutoHide => {
                    if rt.pointer.is_some() {
                        1.0
                    } else {
                        0.0
                    }
                }
                VisibilityMode::Intellihide => {
                    if rt.pointer.is_some() || !self.check_intellihide_overlap(&rt) {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
            rt.physics.reveal_spring.target = target_reveal;
            rt.tick_active = true;
        }
        self.drawing_area.queue_draw();
    }

    fn update_canvas_size(&self) {
        let cfg = self.state.borrow().config.clone();
        let icon_sz = cfg.dock.icon_size as f64;
        let max_scale = if cfg.dock.magnification {
            cfg.dock.magnification_scale
        } else {
            1.0
        };
        let (item_count, sep_count, ws_count, has_media) = {
            let rt = self.runtime.borrow();
            (
                rt.items.len().max(1) as f64,
                rt.separators.len() as f64,
                rt.workspaces.len(),
                rt.media.is_some(),
            )
        };
        let gap = cfg.dock.icon_spacing as f64;
        let sep_w = 12.0;

        let utility_extra = utility_strip_length(&cfg.dock, ws_count, has_media);
        let resting_len = item_count * icon_sz
            + (item_count - 1.0).max(0.0) * gap
            + sep_count * (sep_w + gap)
            + utility_extra;
        let mag_extra = icon_sz * (max_scale - 1.0) * (cfg.dock.magnification_spread * 1.25);
        let padding = 88.0;

        let monitor_limit = resolve_monitor(cfg.dock.monitor_name.as_deref())
            .map(|m| {
                let g = m.geometry();
                match cfg.dock.position {
                    DockPosition::Bottom | DockPosition::Top => (g.width() - 16).max(320) as f64,
                    DockPosition::Left | DockPosition::Right => (g.height() - 16).max(320) as f64,
                }
            })
            .unwrap_or(1900.0);

        let main_len = if matches!(cfg.appearance.style, DockStyle::Panel | DockStyle::EdgeRail) {
            (monitor_limit + 16.0) as i32
        } else {
            (resting_len + mag_extra + padding).clamp(220.0, monitor_limit) as i32
        };
        let cross_len = ((icon_sz * max_scale * 1.15) + cfg.dock.hover_lift + 96.0) as i32;

        let target_wh = match cfg.dock.position {
            DockPosition::Bottom | DockPosition::Top => (main_len, cross_len),
            DockPosition::Left | DockPosition::Right => (cross_len, main_len),
        };

        let changed = {
            let mut rt = self.runtime.borrow_mut();
            if rt.last_canvas_size != target_wh {
                rt.last_canvas_size = target_wh;
                rt.last_input_rect = None;
                true
            } else {
                false
            }
        };

        if changed {
            self.drawing_area.set_content_width(target_wh.0);
            self.drawing_area.set_content_height(target_wh.1);
            self.window.set_default_size(target_wh.0, target_wh.1);
            self.window.queue_resize();
            reposition_x11_dock(
                &self.window,
                cfg.dock.position,
                cfg.appearance.edge_margin as i32,
                target_wh.0,
                target_wh.1,
                cfg.dock.monitor_name.as_deref(),
            );
        }
    }

    pub fn update_windows(&self, windows: Vec<crate::platform::WindowInfo>) {
        let prev_running: HashMap<String, usize> = {
            let rt = self.runtime.borrow();
            rt.items
                .iter()
                .map(|it| (it.key.clone(), it.window_ids.len()))
                .collect()
        };

        self.state.borrow_mut().windows = windows;
        self.rebuild_items();
        self.update_canvas_size();

        {
            let mut rt = self.runtime.borrow_mut();
            let newly_started: Vec<String> = rt
                .items
                .iter()
                .filter(|it| !it.window_ids.is_empty() && prev_running.get(&it.key).copied().unwrap_or(0) == 0)
                .map(|it| it.key.clone())
                .collect();
            for key in newly_started {
                rt.physics.notify_launched(&key);
            }

            let focused_wid: Option<String> = self
                .state
                .borrow()
                .windows
                .iter()
                .find(|w| w.focused)
                .map(|w| w.id.clone());
            if let Some(fwid) = focused_wid {
                if let Some(item_key) = rt
                    .items
                    .iter()
                    .find(|it| it.window_ids.iter().any(|id| id == &fwid))
                    .map(|it| it.key.clone())
                {
                    rt.last_focused_window.insert(item_key, fwid);
                }
            }

            if self.state.borrow().config.dock.visibility == VisibilityMode::Intellihide {
                let overlaps = self.check_intellihide_overlap(&rt);
                if rt.pointer.is_none() {
                    rt.physics.reveal_spring.target = if overlaps { 0.0 } else { 1.0 };
                }
            }
            rt.tick_active = true;
        }
        self.drawing_area.queue_draw();
    }

    fn check_intellihide_overlap(&self, rt: &DockRuntime) -> bool {
        let st = self.state.borrow();
        let monitor_geo =
            resolve_monitor(st.config.dock.monitor_name.as_deref()).map(|m| m.geometry());
        let Some(geo) = monitor_geo else {
            return false;
        };
        let (_, _, pw, ph) = rt.pill_rect;
        let pw = pw.max(200.0) as i32;
        let ph = ph.max(60.0) as i32;
        let dock_rect = match st.config.dock.position {
            DockPosition::Bottom => Rect {
                x: geo.x() + (geo.width() - pw) / 2,
                y: geo.y() + geo.height() - ph - st.config.appearance.edge_margin as i32,
                w: pw,
                h: ph,
            },
            DockPosition::Top => Rect {
                x: geo.x() + (geo.width() - pw) / 2,
                y: geo.y() + st.config.appearance.edge_margin as i32,
                w: pw,
                h: ph,
            },
            DockPosition::Left => Rect {
                x: geo.x() + st.config.appearance.edge_margin as i32,
                y: geo.y() + (geo.height() - ph) / 2,
                w: pw,
                h: ph,
            },
            DockPosition::Right => Rect {
                x: geo.x() + geo.width() - pw - st.config.appearance.edge_margin as i32,
                y: geo.y() + (geo.height() - ph) / 2,
                w: pw,
                h: ph,
            },
        };
        let mon_rect = Rect {
            x: geo.x(),
            y: geo.y(),
            w: geo.width(),
            h: geo.height(),
        };
        any_window_overlaps_mode(
            &st.windows,
            dock_rect,
            mon_rect,
            st.config.dock.intellihide_mode,
        )
    }

    fn rebuild_items(&self) {
        let st = self.state.borrow();
        let mut items = Vec::new();
        let mut separators = Vec::new();

        let mut windows_by_desktop: HashMap<String, Vec<String>> = HashMap::new();
        let mut unpinned_order: Vec<(String, String, String)> = Vec::new();

        for win in &st.windows {
            if st.config.dock.isolate_workspaces && !win.on_active_workspace {
                continue;
            }
            if let Some(entry) = st.desktop_index.match_app_id(&win.app_id) {
                windows_by_desktop
                    .entry(entry.id.clone())
                    .or_default()
                    .push(win.id.clone());
                if st.config.dock.show_unpinned
                    && !st.config.apps.pinned.contains(&entry.id)
                    && !unpinned_order.iter().any(|(k, _, _)| k == &entry.id)
                {
                    unpinned_order.push((
                        entry.id.clone(),
                        entry.name.clone(),
                        entry.icon.clone().unwrap_or_default(),
                    ));
                }
            } else {
                let raw_key = format!("raw:{}", win.app_id.to_lowercase());
                windows_by_desktop
                    .entry(raw_key.clone())
                    .or_default()
                    .push(win.id.clone());
                if st.config.dock.show_unpinned && !unpinned_order.iter().any(|(k, _, _)| k == &raw_key)
                {
                    unpinned_order.push((raw_key, win.app_id.clone(), win.app_id.to_lowercase()));
                }
            }
        }

        for pinned_id in &st.config.apps.pinned {
            let (label, icon) = if let Some(entry) = st.desktop_index.get(pinned_id) {
                (entry.name.clone(), entry.icon.clone().unwrap_or_default())
            } else {
                (
                    pinned_id.trim_end_matches(".desktop").to_string(),
                    String::new(),
                )
            };
            let wids = windows_by_desktop.remove(pinned_id).unwrap_or_default();
            items.push(DockItem {
                key: pinned_id.clone(),
                label,
                icon_name: icon,
                kind: DockItemKind::App {
                    desktop_id: pinned_id.clone(),
                    app_id_fallback: None,
                },
                pinned: true,
                window_ids: wids,
            });
        }

        if st.config.dock.show_launchpad {
            let lp_pos = st.config.dock.launchpad_index.min(items.len());
            items.insert(
                lp_pos,
                DockItem {
                    key: "__launchpad__".into(),
                    label: "Launchpad".into(),
                    icon_name: "view-app-grid-symbolic".into(),
                    kind: DockItemKind::Launchpad,
                    pinned: true,
                    window_ids: Vec::new(),
                },
            );
        }

        if !unpinned_order.is_empty() && !items.is_empty() {
            separators.push(items.len() - 1);
        }

        for (key, label, icon) in unpinned_order {
            let wids = windows_by_desktop.remove(&key).unwrap_or_default();
            let desktop_id = if key.starts_with("raw:") {
                String::new()
            } else {
                key.clone()
            };
            items.push(DockItem {
                key: key.clone(),
                label,
                icon_name: icon,
                kind: DockItemKind::App {
                    desktop_id,
                    app_id_fallback: Some(key),
                },
                pinned: false,
                window_ids: wids,
            });
        }

        if st.config.dock.show_window_tiles && !st.windows.is_empty() {
            if !items.is_empty() {
                separators.push(items.len() - 1);
            }
            for win in st.windows.iter().take(6) {
                let icon_name = st
                    .desktop_index
                    .match_app_id(&win.app_id)
                    .and_then(|e| e.icon.clone())
                    .unwrap_or_else(|| win.app_id.to_lowercase());
                let title = if win.title.is_empty() {
                    win.app_id.clone()
                } else {
                    win.title.clone()
                };
                items.push(DockItem {
                    key: format!("win:{}", win.id),
                    label: title.clone(),
                    icon_name,
                    kind: DockItemKind::WindowTile {
                        window_id: win.id.clone(),
                        title,
                    },
                    pinned: false,
                    window_ids: if win.focused {
                        vec![win.id.clone()]
                    } else {
                        Vec::new()
                    },
                });
            }
        }

        let has_right_section = !st.config.apps.pinned_folders.is_empty() || st.config.dock.show_trash;
        if has_right_section && !items.is_empty() {
            separators.push(items.len() - 1);
        }

        for folder_path in &st.config.apps.pinned_folders {
            let p = std::path::Path::new(folder_path);
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(folder_path)
                .to_string();
            let lower = name.to_lowercase();
            let icon_name = if lower.contains("download") {
                "folder-download"
            } else if lower.contains("document") {
                "folder-documents"
            } else if lower.contains("picture") || lower.contains("photo") {
                "folder-pictures"
            } else if lower.contains("music") {
                "folder-music"
            } else if lower.contains("video") {
                "folder-videos"
            } else {
                "folder"
            };
            items.push(DockItem {
                key: format!("folder:{folder_path}"),
                label: name,
                icon_name: icon_name.to_string(),
                kind: DockItemKind::Folder {
                    path: folder_path.clone(),
                },
                pinned: true,
                window_ids: Vec::new(),
            });
        }

        if st.config.dock.show_trash {
            let trash_count = st.trash.item_count();
            let trash_label = if trash_count == 0 {
                "Trash".to_string()
            } else if trash_count == 1 {
                "Trash (1 item)".to_string()
            } else {
                format!("Trash ({trash_count} items)")
            };
            items.push(DockItem {
                key: "__trash__".into(),
                label: trash_label,
                icon_name: if trash_count == 0 {
                    "user-trash".into()
                } else {
                    "user-trash-full".into()
                },
                kind: DockItemKind::Trash,
                pinned: true,
                window_ids: Vec::new(),
            });
        }

        let keys: Vec<String> = items.iter().map(|i| i.key.clone()).collect();
        let mut rt = self.runtime.borrow_mut();
        for item in &items {
            let is_new_unpinned = !item.pinned && !rt.physics.items.contains_key(&item.key);
            rt.physics.ensure_item(&item.key, is_new_unpinned);
        }
        rt.physics.retain_keys(&keys);
        rt.items = items;
        rt.separators = separators;
    }

    fn setup_tick(&self) {
        let state = self.state.clone();
        let runtime = self.runtime.clone();
        let window = self.window.clone();
        let area = self.drawing_area.clone();

        area.add_tick_callback(move |da, _clock| {
            let now = Instant::now();
            let cfg = state.borrow().config.clone();
            let mut rt = runtime.borrow_mut();
            let dt = rt
                .last_frame
                .map(|prev| now.duration_since(prev).as_secs_f64())
                .unwrap_or(1.0 / 60.0);
            rt.last_frame = Some(now);

            let icon_size = cfg.dock.icon_size as f64;
            let max_scale = if cfg.dock.magnification {
                cfg.dock.magnification_scale
            } else {
                1.0
            };
            let gap = cfg.dock.icon_spacing as f64;
            let sep_w = 12.0;

            let resting_scales = vec![1.0; rt.items.len()];
            let (resting_slots, resting_len) =
                layout_slots(&resting_scales, icon_size, gap, &rt.separators, sep_w);

            let da_w = da.width() as f64;
            let da_h = da.height() as f64;
            if da_w < 10.0 || da_h < 10.0 {
                return glib::ControlFlow::Continue;
            }
            let main_total = match cfg.dock.position {
                DockPosition::Bottom | DockPosition::Top => da_w,
                DockPosition::Left | DockPosition::Right => da_h,
            };
            let clock_extra =
                utility_strip_length(&cfg.dock, rt.workspaces.len(), rt.media.is_some());
            let resting_origin = (main_total - (resting_len + clock_extra)) * 0.5;
            let centers: Vec<f64> = resting_slots
                .iter()
                .map(|s| resting_origin + s.center)
                .collect();

            let ptr_axis = rt.pointer.map(|(px, py)| match cfg.dock.position {
                DockPosition::Bottom | DockPosition::Top => px,
                DockPosition::Left | DockPosition::Right => py,
            });
            let dragging = rt.drag.as_ref().map(|d| d.active).unwrap_or(false);
            rt.physics.update_pointer_target(if dragging { None } else { ptr_axis });
            let smoothed_ptr = if !dragging && rt.physics.pointer_initialized {
                Some(rt.physics.pointer_spring.value)
            } else {
                None
            };
            let hover_factor = if dragging { 0.0 } else { rt.physics.hover_spring.value };
            let is_playing_media =
                cfg.dock.show_media && rt.media.as_ref().map(|m| m.playing).unwrap_or(false);
            if (rt.pointer.is_some() && !dragging) || is_playing_media {
                rt.hover_time += dt.min(0.05);
            }
            let ptr_vel = if !dragging && rt.physics.pointer_initialized {
                rt.physics.pointer_spring.velocity
            } else {
                0.0
            };
            let hover_targets = compute_hover_targets(
                &centers,
                smoothed_ptr,
                icon_size,
                gap,
                max_scale,
                hover_factor,
                cfg.dock.magnification_spread,
                cfg.dock.hover_animation,
                ptr_vel,
                rt.hover_time,
            );

            let drag_info = rt
                .drag
                .as_ref()
                .filter(|d| d.active)
                .map(|d| (d.item_index, match cfg.dock.position {
                    DockPosition::Bottom | DockPosition::Top => d.current_x,
                    DockPosition::Left | DockPosition::Right => d.current_y,
                }));

            let item_meta: Vec<(String, bool, bool)> = rt
                .items
                .iter()
                .map(|it| {
                    (
                        it.key.clone(),
                        it.pinned,
                        matches!(it.kind, DockItemKind::App { .. } | DockItemKind::Launchpad),
                    )
                })
                .collect();

            let max_delta = (max_scale - 1.0).max(0.01);
            let extra_lift = cfg.dock.hover_lift.clamp(0.0, 48.0);
            for (idx, (key, pinned, is_app)) in item_meta.iter().enumerate() {
                let target = hover_targets
                    .get(idx)
                    .copied()
                    .unwrap_or(HoverTarget::REST);
                let elev_norm = ((target.scale - 1.0) / max_delta).clamp(0.0, 1.25);
                let smooth_elev = elev_norm * elev_norm * (3.0 - 2.0 * elev_norm.min(1.0));
                let anim = rt.physics.ensure_item(key, false);
                anim.scale.target = target.scale;
                anim.hover_sx.target = target.scale_x;
                anim.hover_sy.target = target.scale_y;
                anim.hover_skew.target = target.skew;
                anim.hover_lift.target = target.lift + extra_lift * smooth_elev;
                anim.hover_shift.target = target.shift;
                anim.hover_angle.target = target.angle;
                anim.hover_glow.target = target.glow;

                if let Some((drag_idx, drag_pos)) = drag_info {
                    if idx != drag_idx && *pinned && *is_app {
                        let slot_step = icon_size + gap;
                        let c = centers.get(idx).copied().unwrap_or(0.0);
                        if drag_idx < idx && drag_pos > c {
                            anim.drag_shift.target = -slot_step;
                        } else if drag_idx > idx && drag_pos < c {
                            anim.drag_shift.target = slot_step;
                        } else {
                            anim.drag_shift.target = 0.0;
                        }
                    } else {
                        anim.drag_shift.target = 0.0;
                    }
                } else {
                    anim.drag_shift.target = 0.0;
                }
            }

            rt.physics.step(dt, icon_size);

            let animated_scales: Vec<f64> = rt
                .items
                .iter()
                .map(|it| {
                    rt.physics
                        .items
                        .get(&it.key)
                        .map(|a| a.scale.value * a.presence.value)
                        .unwrap_or(1.0)
                })
                .collect();
            let (mut slots, total_len) =
                layout_slots(&animated_scales, icon_size, gap, &rt.separators, sep_w);
            let content_len = total_len + clock_extra;
            let origin = (main_total - content_len) * 0.5;
            for (idx, slot) in slots.iter_mut().enumerate() {
                let (shift, h_shift) = rt
                    .items
                    .get(idx)
                    .and_then(|it| rt.physics.items.get(&it.key))
                    .map(|a| (a.drag_shift.value, a.hover_shift.value))
                    .unwrap_or((0.0, 0.0));
                slot.center += origin + shift + h_shift;
            }

            rt.hovered_index = if let Some(p_axis) = ptr_axis {
                if !dragging && rt.physics.reveal_spring.value > 0.5 {
                    slots.iter().enumerate().find_map(|(i, s)| {
                        if (p_axis - s.center).abs() <= (s.size + gap) * 0.5 {
                            Some(i)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            } else {
                None
            };

            let (pad_main, pad_cross) = match cfg.appearance.style {
                DockStyle::Attached => (22.0, 14.0),
                DockStyle::Capsule => (24.0, 12.0),
                DockStyle::Glass3D => (26.0, 10.0),
                DockStyle::Floating
                | DockStyle::Panel
                | DockStyle::EdgeRail
                | DockStyle::SplitIslands
                | DockStyle::BaselineBar
                | DockStyle::Aurora
                | DockStyle::CyberNeon
                | DockStyle::Invisible => (14.0, 10.0),
            };
            let pill_cross = icon_size + pad_cross * 2.0;
            let reveal = rt.physics.reveal_spring.value.clamp(0.0, 1.0);
            let hidden_offset = (pill_cross + cfg.appearance.edge_margin as f64 + 16.0) * (1.0 - reveal);

            let pill_rect = match (cfg.dock.position, cfg.appearance.style) {
                (DockPosition::Bottom, DockStyle::Panel | DockStyle::EdgeRail) => {
                    (0.0, da_h - pill_cross + hidden_offset, da_w, pill_cross)
                }
                (DockPosition::Top, DockStyle::Panel | DockStyle::EdgeRail) => {
                    (0.0, -hidden_offset, da_w, pill_cross)
                }
                (DockPosition::Left, DockStyle::Panel | DockStyle::EdgeRail) => {
                    (-hidden_offset, 0.0, pill_cross, da_h)
                }
                (DockPosition::Right, DockStyle::Panel | DockStyle::EdgeRail) => {
                    (da_w - pill_cross + hidden_offset, 0.0, pill_cross, da_h)
                }
                (DockPosition::Bottom, _) => (
                    origin - pad_main,
                    da_h - pill_cross + hidden_offset,
                    content_len + pad_main * 2.0,
                    pill_cross,
                ),
                (DockPosition::Top, _) => (
                    origin - pad_main,
                    -hidden_offset,
                    content_len + pad_main * 2.0,
                    pill_cross,
                ),
                (DockPosition::Left, _) => (
                    -hidden_offset,
                    origin - pad_main,
                    pill_cross,
                    content_len + pad_main * 2.0,
                ),
                (DockPosition::Right, _) => (
                    da_w - pill_cross + hidden_offset,
                    origin - pad_main,
                    pill_cross,
                    content_len + pad_main * 2.0,
                ),
            };
            rt.pill_rect = pill_rect;
            rt.slots = slots;

            let input_rect = if reveal < 0.08 {
                match cfg.dock.position {
                    DockPosition::Bottom => Rect {
                        x: (pill_rect.0 as i32).max(0),
                        y: (da_h as i32 - 6).max(0),
                        w: (pill_rect.2 as i32).max(160),
                        h: 6,
                    },
                    DockPosition::Top => Rect {
                        x: (pill_rect.0 as i32).max(0),
                        y: 0,
                        w: (pill_rect.2 as i32).max(160),
                        h: 6,
                    },
                    DockPosition::Left => Rect {
                        x: 0,
                        y: (pill_rect.1 as i32).max(0),
                        w: 6,
                        h: (pill_rect.3 as i32).max(160),
                    },
                    DockPosition::Right => Rect {
                        x: (da_w as i32 - 6).max(0),
                        y: (pill_rect.1 as i32).max(0),
                        w: 6,
                        h: (pill_rect.3 as i32).max(160),
                    },
                }
            } else {
                let extra = if rt.pointer.is_some() {
                    (icon_size * (max_scale - 1.0) + cfg.dock.hover_lift) as i32 + 38
                } else {
                    4
                };
                match cfg.dock.position {
                    DockPosition::Bottom => Rect {
                        x: (pill_rect.0 as i32 - 8).max(0),
                        y: (pill_rect.1 as i32 - extra).max(0),
                        w: pill_rect.2 as i32 + 16,
                        h: pill_rect.3 as i32 + extra + 8,
                    },
                    DockPosition::Top => Rect {
                        x: (pill_rect.0 as i32 - 8).max(0),
                        y: 0,
                        w: pill_rect.2 as i32 + 16,
                        h: pill_rect.3 as i32 + extra + 8,
                    },
                    DockPosition::Left => Rect {
                        x: 0,
                        y: (pill_rect.1 as i32 - 8).max(0),
                        w: pill_rect.2 as i32 + extra + 8,
                        h: pill_rect.3 as i32 + 16,
                    },
                    DockPosition::Right => Rect {
                        x: (pill_rect.0 as i32 - extra).max(0),
                        y: (pill_rect.1 as i32 - 8).max(0),
                        w: pill_rect.2 as i32 + extra + 8,
                        h: pill_rect.3 as i32 + 16,
                    },
                }
            };

            if rt.last_input_rect != Some(input_rect) {
                update_input_region(&window, Some(input_rect));
                rt.last_input_rect = Some(input_rect);
            }

            let is_dynamic_neon =
                matches!(cfg.appearance.style, DockStyle::CyberNeon | DockStyle::Aurora);
            if is_dynamic_neon {
                let speed = cfg.appearance.neon_speed.clamp(0.2, 3.5);
                rt.neon_phase = (rt.neon_phase + dt.min(0.05) * speed * 0.16).rem_euclid(1.0);
            }

            let is_live_ripple =
                cfg.dock.hover_animation == HoverAnimation::PulseWave && rt.pointer.is_some();
            let settled = rt.physics.is_settled()
                && !dragging
                && !is_dynamic_neon
                && !is_live_ripple
                && !is_playing_media;
            if !settled || rt.tick_active {
                da.queue_draw();
                if settled {
                    rt.tick_active = false;
                }
            }

            glib::ControlFlow::Continue
        });
    }

    fn setup_drawing(&self) {
        let state = self.state.clone();
        let runtime = self.runtime.clone();

        self.drawing_area.set_draw_func(move |_da, cr, w, h| {
            let cfg = state.borrow().config.clone();
            let mut rt = runtime.borrow_mut();

            cr.set_operator(cairo::Operator::Clear);
            let _ = cr.paint();
            cr.set_operator(cairo::Operator::Over);

            if rt.items.is_empty() {
                return;
            }

            let effective_opacity = if cfg.appearance.dynamic_opacity {
                let has_active_win = state.borrow().windows.iter().any(|w| w.on_active_workspace);
                if has_active_win {
                    (cfg.appearance.opacity + 0.20).min(0.96)
                } else {
                    (cfg.appearance.opacity * 0.65).max(0.22)
                }
            } else {
                cfg.appearance.opacity
            };
            let mut pal = glass_palette(
                cfg.appearance.theme,
                effective_opacity,
                cfg.appearance.custom_tint.as_deref(),
                cfg.appearance.custom_indicator.as_deref(),
            );
            if cfg.appearance.pywal_theme {
                if let Some(((br, bg, bb), (ar, ag, ab))) = load_pywal_colors() {
                    pal.bg = (br, bg, bb, pal.bg.3);
                    pal.dot = (ar, ag, ab, 0.96);
                }
            }
            let bw = cfg.appearance.border_width.clamp(0.0, 3.5);
            let (px, py, pw, ph) = rt.pill_rect;
            let radius = (cfg.appearance.corner_radius as f64)
                .min(pw * 0.45)
                .min(ph * 0.45);

            match cfg.appearance.style {
                DockStyle::Floating => {

                    for i in 1..=3 {
                        let spread = i as f64 * 2.0;
                        rounded_rect(
                            cr,
                            px - spread * 0.4,
                            py + spread * 0.5,
                            pw + spread * 0.8,
                            ph + spread * 0.6,
                            radius + spread * 0.3,
                        );
                        cr.set_source_rgba(0.0, 0.0, 0.0, 0.10 / i as f64);
                        let _ = cr.fill();
                    }

                    rounded_rect(cr, px, py, pw, ph, radius);
                    cr.set_source_rgba(pal.bg.0, pal.bg.1, pal.bg.2, pal.bg.3);
                    let _ = cr.fill_preserve();
                    if bw > 0.05 {
                        cr.set_source_rgba(
                            pal.border_outer.0,
                            pal.border_outer.1,
                            pal.border_outer.2,
                            pal.border_outer.3,
                        );
                        cr.set_line_width(bw);
                        let _ = cr.stroke();
                    } else {
                        cr.new_path();
                    }

                    if bw > 0.05 && pw > 4.0 && ph > 4.0 {
                        rounded_rect(
                            cr,
                            px + 1.0,
                            py + 1.0,
                            pw - 2.0,
                            ph - 2.0,
                            (radius - 1.0).max(0.0),
                        );
                        cr.set_source_rgba(
                            pal.border_inner.0,
                            pal.border_inner.1,
                            pal.border_inner.2,
                            pal.border_inner.3,
                        );
                        cr.set_line_width(bw);
                        let _ = cr.stroke();
                    }
                }
                DockStyle::Attached => {

                    attached_shelf_path(cr, px, py, pw, ph, radius, cfg.dock.position);
                    let grad = match cfg.dock.position {
                        DockPosition::Bottom => cairo::LinearGradient::new(px, py, px, py + ph),
                        DockPosition::Top => cairo::LinearGradient::new(px, py + ph, px, py),
                        DockPosition::Left => cairo::LinearGradient::new(px + pw, py, px, py),
                        DockPosition::Right => cairo::LinearGradient::new(px, py, px + pw, py),
                    };
                    grad.add_color_stop_rgba(
                        0.0,
                        (pal.bg.0 + 0.04).min(1.0),
                        (pal.bg.1 + 0.03).min(1.0),
                        (pal.bg.2 + 0.02).min(1.0),
                        (pal.bg.3 * 0.96).clamp(0.15, 1.0),
                    );
                    grad.add_color_stop_rgba(
                        1.0,
                        pal.bg.0,
                        pal.bg.1,
                        pal.bg.2,
                        (pal.bg.3 * 0.78).clamp(0.12, 1.0),
                    );
                    let _ = cr.set_source(&grad);
                    let _ = cr.fill_preserve();
                    cr.set_source_rgba(
                        pal.border_inner.0,
                        pal.border_inner.1,
                        pal.border_inner.2,
                        pal.border_inner.3 * 0.55,
                    );
                    cr.set_line_width(1.0);
                    let _ = cr.stroke();
                }
                DockStyle::Panel => {
                    cr.rectangle(px, py, pw, ph);
                    cr.set_source_rgba(pal.bg.0, pal.bg.1, pal.bg.2, pal.bg.3);
                    let _ = cr.fill();
                    cr.set_source_rgba(
                        pal.border_inner.0,
                        pal.border_inner.1,
                        pal.border_inner.2,
                        pal.border_inner.3 * 0.75,
                    );
                    cr.set_line_width(1.0);
                    match cfg.dock.position {
                        DockPosition::Bottom => {
                            cr.move_to(px, py + 0.5);
                            cr.line_to(px + pw, py + 0.5);
                        }
                        DockPosition::Top => {
                            cr.move_to(px, py + ph - 0.5);
                            cr.line_to(px + pw, py + ph - 0.5);
                        }
                        DockPosition::Left => {
                            cr.move_to(px + pw - 0.5, py);
                            cr.line_to(px + pw - 0.5, py + ph);
                        }
                        DockPosition::Right => {
                            cr.move_to(px + 0.5, py);
                            cr.line_to(px + 0.5, py + ph);
                        }
                    }
                    let _ = cr.stroke();
                }
                DockStyle::Glass3D => {
                    draw_glass_3d_deck(cr, px, py, pw, ph, cfg.dock.position, pal);
                }
                DockStyle::CyberNeon => {
                    let neon_r = radius.max(14.0);
                    let phase = rt.neon_phase.rem_euclid(1.0);
                    let (gx0, gy0, gx1, gy1) = match cfg.dock.position {
                        DockPosition::Bottom | DockPosition::Top => (px, py, px + pw, py),
                        DockPosition::Left | DockPosition::Right => (px, py, px, py + ph),
                    };
                    let add_neon_stops = |grad: &cairo::LinearGradient, alpha: f64| {
                        for k in 0..=6 {
                            let t = k as f64 / 6.0;
                            let hue = (phase - t).rem_euclid(1.0);
                            let c = hsv_to_rgb(hue, 0.94, 1.0);
                            grad.add_color_stop_rgba(t, c.0, c.1, c.2, alpha);
                        }
                    };

                    for i in 1..=3 {
                        let sp = i as f64 * 2.6;
                        rounded_rect(
                            cr,
                            px - sp,
                            py - sp,
                            pw + sp * 2.0,
                            ph + sp * 2.0,
                            neon_r + sp * 0.5,
                        );
                        let glow = cairo::LinearGradient::new(gx0, gy0, gx1, gy1);
                        add_neon_stops(&glow, 0.18 / i as f64);
                        let _ = cr.set_source(&glow);
                        cr.set_line_width(2.2);
                        let _ = cr.stroke();
                    }

                    rounded_rect(cr, px, py, pw, ph, neon_r);
                    cr.set_source_rgba(0.05, 0.06, 0.10, pal.bg.3);
                    let _ = cr.fill_preserve();

                    let rim = cairo::LinearGradient::new(gx0, gy0, gx1, gy1);
                    add_neon_stops(&rim, 0.96);
                    let _ = cr.set_source(&rim);
                    cr.set_line_width(2.0);
                    let _ = cr.stroke();
                }
                DockStyle::Capsule => {
                    let cap_r = (pw.min(ph) * 0.5).max(16.0);
                    if pal.bg.3 > 0.05 {
                        for i in 1..=3 {
                            let sp = i as f64 * 2.0;
                            rounded_rect(
                                cr,
                                px - sp * 0.3,
                                py + sp * 0.5,
                                pw + sp * 0.6,
                                ph + sp * 0.5,
                                cap_r,
                            );
                            cr.set_source_rgba(0.0, 0.0, 0.0, (0.12 * pal.bg.3) / i as f64);
                            let _ = cr.fill();
                        }
                    }

                    rounded_rect(cr, px, py, pw, ph, cap_r);
                    cr.set_source_rgba(pal.bg.0, pal.bg.1, pal.bg.2, pal.bg.3);
                    let _ = cr.fill_preserve();
                    cr.set_source_rgba(
                        pal.border_inner.0,
                        pal.border_inner.1,
                        pal.border_inner.2,
                        0.26,
                    );
                    cr.set_line_width(1.2);
                    let _ = cr.stroke();
                }
                DockStyle::SplitIslands => {
                    let is_horiz = matches!(
                        cfg.dock.position,
                        DockPosition::Bottom | DockPosition::Top
                    );
                    let mut groups: Vec<(usize, usize)> = Vec::new();
                    let mut g_start = 0_usize;
                    for &sep_idx in &rt.separators {
                        if sep_idx < rt.slots.len() && sep_idx >= g_start {
                            groups.push((g_start, sep_idx));
                            g_start = sep_idx + 1;
                        }
                    }
                    if g_start < rt.slots.len() {
                        groups.push((g_start, rt.slots.len() - 1));
                    }
                    let pad = 10.0;
                    for (s_idx, e_idx) in groups {
                        if let (Some(first), Some(last)) = (rt.slots.get(s_idx), rt.slots.get(e_idx)) {
                            let m0 = first.center - first.size * 0.5 - pad;
                            let m1 = last.center + last.size * 0.5 + pad;
                            let (ix, iy, iw, ih) = if is_horiz {
                                (m0, py, (m1 - m0).max(24.0), ph)
                            } else {
                                (px, m0, pw, (m1 - m0).max(24.0))
                            };
                            let ir = radius.min(iw * 0.45).min(ih * 0.45);
                            rounded_rect(cr, ix, iy, iw, ih, ir);
                            cr.set_source_rgba(pal.bg.0, pal.bg.1, pal.bg.2, pal.bg.3);
                            let _ = cr.fill_preserve();
                            if bw > 0.05 {
                                cr.set_source_rgba(
                                    pal.border_inner.0,
                                    pal.border_inner.1,
                                    pal.border_inner.2,
                                    pal.border_inner.3,
                                );
                                cr.set_line_width(bw);
                                let _ = cr.stroke();
                            } else {
                                cr.new_path();
                            }
                        }
                    }
                }
                DockStyle::BaselineBar => {
                    let bar_thick = 5.0;
                    let (bx, by, bw_len, bh_len) = match cfg.dock.position {
                        DockPosition::Bottom => (px + 8.0, py + ph - bar_thick - 2.0, (pw - 16.0).max(40.0), bar_thick),
                        DockPosition::Top => (px + 8.0, py + 2.0, (pw - 16.0).max(40.0), bar_thick),
                        DockPosition::Left => (px + 2.0, py + 8.0, bar_thick, (ph - 16.0).max(40.0)),
                        DockPosition::Right => (px + pw - bar_thick - 2.0, py + 8.0, bar_thick, (ph - 16.0).max(40.0)),
                    };
                    rounded_rect(cr, bx, by, bw_len, bh_len, bar_thick * 0.5);
                    cr.set_source_rgba(pal.bg.0, pal.bg.1, pal.bg.2, (pal.bg.3 + 0.15).min(0.92));
                    let _ = cr.fill_preserve();
                    cr.set_source_rgba(
                        pal.border_inner.0,
                        pal.border_inner.1,
                        pal.border_inner.2,
                        0.42,
                    );
                    cr.set_line_width(1.0);
                    let _ = cr.stroke();
                }
                DockStyle::Aurora => {
                    let phase = rt.neon_phase.rem_euclid(1.0);
                    let (gx0, gy0, gx1, gy1) = match cfg.dock.position {
                        DockPosition::Bottom | DockPosition::Top => (px, py, px + pw, py),
                        DockPosition::Left | DockPosition::Right => (px, py, px, py + ph),
                    };
                    rounded_rect(cr, px, py, pw, ph, radius);
                    cr.set_source_rgba(0.08, 0.09, 0.13, pal.bg.3.clamp(0.45, 0.90));
                    let _ = cr.fill_preserve();

                    let wash = cairo::LinearGradient::new(gx0, gy0, gx1, gy1);
                    let h0 = (0.50 + 0.10 * (phase * PI * 2.0).sin()).rem_euclid(1.0);
                    let h1 = (0.64 + 0.09 * (phase * PI * 2.0 + 2.0).sin()).rem_euclid(1.0);
                    let h2 = (0.78 + 0.08 * (phase * PI * 2.0 + 4.0).sin()).rem_euclid(1.0);
                    let c0 = hsv_to_rgb(h0, 0.42, 0.72);
                    let c1 = hsv_to_rgb(h1, 0.36, 0.68);
                    let c2 = hsv_to_rgb(h2, 0.40, 0.70);
                    wash.add_color_stop_rgba(0.0, c0.0, c0.1, c0.2, 0.24);
                    wash.add_color_stop_rgba(0.5, c1.0, c1.1, c1.2, 0.20);
                    wash.add_color_stop_rgba(1.0, c2.0, c2.1, c2.2, 0.24);
                    let _ = cr.set_source(&wash);
                    let _ = cr.fill_preserve();

                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.22);
                    cr.set_line_width(bw.max(1.0));
                    let _ = cr.stroke();
                }
                DockStyle::EdgeRail => {
                    cr.rectangle(px, py, pw, ph);
                    cr.set_source_rgba(pal.bg.0, pal.bg.1, pal.bg.2, pal.bg.3.max(0.76));
                    let _ = cr.fill();

                    let (lx0, ly0, lx1, ly1) = match cfg.dock.position {
                        DockPosition::Bottom => (px, py + 0.75, px + pw, py + 0.75),
                        DockPosition::Top => (px, py + ph - 0.75, px + pw, py + ph - 0.75),
                        DockPosition::Left => (px + pw - 0.75, py, px + pw - 0.75, py + ph),
                        DockPosition::Right => (px + 0.75, py, px + 0.75, py + ph),
                    };
                    let gleam = cairo::LinearGradient::new(lx0, ly0, lx1, ly1);
                    gleam.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.08);
                    gleam.add_color_stop_rgba(0.5, pal.dot.0, pal.dot.1, pal.dot.2, 0.68);
                    gleam.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.08);
                    let _ = cr.set_source(&gleam);
                    cr.set_line_width(1.5);
                    cr.move_to(lx0, ly0);
                    cr.line_to(lx1, ly1);
                    let _ = cr.stroke();
                }
                DockStyle::Invisible => {

                }
            }

            if cfg.appearance.cursor_sheen && cfg.appearance.style != DockStyle::Invisible {
                if let Some((mx, my)) = rt.pointer {
                    draw_cursor_sheen(cr, (px, py, pw, ph), radius, (mx, my), cfg.appearance.style);
                }
            }

            let icon_base = cfg.dock.icon_size as f64;
            let max_icon_px = (icon_base * cfg.dock.magnification_scale * 2.0).ceil() as i32;
            let gap = cfg.dock.icon_spacing as f64;

            if cfg.appearance.show_separators {
                cr.set_source_rgba(pal.separator.0, pal.separator.1, pal.separator.2, pal.separator.3);
                cr.set_line_width(1.0);
                for &sep_idx in &rt.separators {
                    if let Some(slot) = rt.slots.get(sep_idx) {
                        let pos_main = slot.center + slot.size * 0.5 + gap + 6.0;
                        match cfg.dock.position {
                            DockPosition::Bottom | DockPosition::Top => {
                                cr.move_to(pos_main, py + 12.0);
                                cr.line_to(pos_main, py + ph - 12.0);
                            }
                            DockPosition::Left | DockPosition::Right => {
                                cr.move_to(px + 12.0, pos_main);
                                cr.line_to(px + pw - 12.0, pos_main);
                            }
                        }
                        let _ = cr.stroke();
                    }
                }
            }

            let drag_idx = rt.drag.as_ref().filter(|d| d.active).map(|d| d.item_index);
            let item_count = rt.items.len();

            for pass in 0..2 {
                for idx in 0..item_count {
                    let is_dragged = drag_idx == Some(idx);
                    if (pass == 0 && is_dragged) || (pass == 1 && !is_dragged) {
                        continue;
                    }
                    let Some(slot) = rt.slots.get(idx).copied() else {
                        continue;
                    };
                    let item = rt.items[idx].clone();
                    let (pose, hover_lift, hover_angle, hover_glow, hover_sx, hover_sy, hover_skew) =
                        rt.physics
                            .items
                            .get(&item.key)
                            .map(|a| {
                                (
                                    a.launch_pose,
                                    a.hover_lift.value,
                                    a.hover_angle.value,
                                    a.hover_glow.value,
                                    a.hover_sx.value,
                                    a.hover_sy.value,
                                    a.hover_skew.value,
                                )
                            })
                            .unwrap_or((
                                Default::default(),
                                0.0,
                                0.0,
                                0.0,
                                1.0,
                                1.0,
                                0.0,
                            ));
                    let bounce_px = pose.offset_y + hover_lift;

                    let sz = slot.size;
                    if sz < 2.0 {
                        continue;
                    }

                    let edge_pad = match cfg.appearance.style {
                        DockStyle::Attached => 14.0,
                        DockStyle::Capsule => 12.0,
                        DockStyle::Glass3D => 12.0,
                        DockStyle::Floating
                        | DockStyle::Panel
                        | DockStyle::EdgeRail
                        | DockStyle::SplitIslands
                        | DockStyle::BaselineBar
                        | DockStyle::Aurora
                        | DockStyle::CyberNeon
                        | DockStyle::Invisible => 10.0,
                    };
                    let (mut ix, mut iy) = match cfg.dock.position {
                        DockPosition::Bottom => (
                            slot.center - sz * 0.5 + pose.offset_x,
                            py + ph - edge_pad - sz - bounce_px,
                        ),
                        DockPosition::Top => (
                            slot.center - sz * 0.5 + pose.offset_x,
                            py + edge_pad + bounce_px,
                        ),
                        DockPosition::Left => (
                            px + edge_pad + bounce_px,
                            slot.center - sz * 0.5 + pose.offset_x,
                        ),
                        DockPosition::Right => (
                            px + pw - edge_pad - sz - bounce_px,
                            slot.center - sz * 0.5 + pose.offset_x,
                        ),
                    };

                    if is_dragged {
                        if let Some(d) = rt.drag.as_ref() {
                            ix = d.current_x - sz * 0.5;
                            iy = d.current_y - sz * 0.5;
                        }
                    }

                    let is_hovered_item = rt.hovered_index == Some(idx) && !is_dragged;
                    let adaptive_rgb = if cfg.appearance.adaptive_aura && !item.icon_name.is_empty() {
                        rt.icon_cache.dominant_color(&item.icon_name, 48)
                    } else {
                        None
                    };

                    if cfg.appearance.hover_highlight && is_hovered_item {
                        let halo_pad = 6.0;
                        rounded_rect(
                            cr,
                            ix - halo_pad,
                            iy - halo_pad,
                            sz + halo_pad * 2.0,
                            sz + halo_pad * 2.0,
                            sz * 0.24,
                        );
                        if let Some((ar, ag, ab)) = adaptive_rgb {
                            cr.set_source_rgba(ar, ag, ab, 0.20);
                        } else {
                            cr.set_source_rgba(1.0, 1.0, 1.0, 0.13);
                        }
                        let _ = cr.fill();
                    }

                    let aura_strength = pose
                        .glow
                        .max(hover_glow * 0.72)
                        .max(if is_hovered_item && adaptive_rgb.is_some() { 0.58 } else { 0.0 });
                    if aura_strength > 0.02 && !is_dragged {
                        let (ar, ag, ab) = adaptive_rgb.unwrap_or((0.28, 0.78, 1.0));
                        let cx = ix + sz * 0.5;
                        let cy = iy + sz * 0.5;
                        let rad = sz * 0.82;
                        let aura = cairo::RadialGradient::new(cx, cy, sz * 0.10, cx, cy, rad);
                        aura.add_color_stop_rgba(0.0, ar, ag, ab, aura_strength * 0.58);
                        aura.add_color_stop_rgba(1.0, ar, ag, ab, 0.0);
                        let _ = cr.set_source(&aura);
                        cr.arc(cx, cy, rad, 0.0, PI * 2.0);
                        let _ = cr.fill();
                    }

                    let _ = cr.save();
                    let cx = ix + sz * 0.5;
                    let cy = iy + sz * 0.5;
                    cr.translate(cx, cy);
                    let total_rot = pose.rotation + hover_angle;
                    if total_rot.abs() > 1e-4 {
                        cr.rotate(total_rot);
                    }
                    if hover_skew.abs() > 1e-4 {
                        let mat = match cfg.dock.position {
                            DockPosition::Bottom | DockPosition::Top => {
                                cairo::Matrix::new(1.0, hover_skew, 0.0, 1.0, 0.0, 0.0)
                            }
                            DockPosition::Left | DockPosition::Right => {
                                cairo::Matrix::new(1.0, 0.0, hover_skew, 1.0, 0.0, 0.0)
                            }
                        };
                        cr.transform(mat);
                    }
                    let final_sx = pose.scale_x * hover_sx;
                    let final_sy = pose.scale_y * hover_sy;
                    if (final_sx - 1.0).abs() > 1e-4 || (final_sy - 1.0).abs() > 1e-4 {
                        cr.scale(final_sx, final_sy);
                    }
                    cr.translate(-cx, -cy);

                    if cfg.appearance.icon_shadow {
                        let _ = cr.save();
                        let sx = ix + sz * 0.5;
                        let sy = iy + sz - 1.0;
                        cr.translate(sx, sy);
                        cr.scale(1.0, 0.28);
                        let r = sz * 0.44;
                        let shad = cairo::RadialGradient::new(0.0, 0.0, 0.0, 0.0, 0.0, r);
                        shad.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.36);
                        shad.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
                        let _ = cr.set_source(&shad);
                        cr.arc(0.0, 0.0, r, 0.0, PI * 2.0);
                        let _ = cr.fill();
                        let _ = cr.restore();
                    }

                    if matches!(item.kind, DockItemKind::Launchpad) {
                        draw_launchpad_icon(cr, ix, iy, sz);
                    } else if matches!(item.kind, DockItemKind::WindowTile { .. }) {
                        let pb = rt.icon_cache.get(&item.icon_name, max_icon_px);
                        let is_focused = !item.window_ids.is_empty();
                        draw_window_tile(cr, ix, iy, sz, pb.as_ref(), is_focused, pal);
                    } else if let Some(pb) = rt.icon_cache.get(&item.icon_name, max_icon_px) {
                        let _ = cr.save();
                        cr.translate(ix, iy);
                        let sx = sz / pb.width().max(1) as f64;
                        let sy = sz / pb.height().max(1) as f64;
                        cr.scale(sx, sy);
                        cr.set_source_pixbuf(&pb, 0.0, 0.0);
                        let _ = cr.paint();
                        let _ = cr.restore();
                    }
                    let _ = cr.restore();

                    if cfg.dock.show_window_badges
                        && item.window_ids.len() > 1
                        && !is_dragged
                        && matches!(item.kind, DockItemKind::App { .. })
                    {
                        draw_window_count_badge(cr, ix, iy, sz, item.window_ids.len(), adaptive_rgb);
                    }

                    if cfg.dock.show_indicators && !item.window_ids.is_empty() && !is_dragged {
                        let (dx, dy) = match cfg.dock.position {
                            DockPosition::Bottom => (slot.center, py + ph - 4.5),
                            DockPosition::Top => (slot.center, py + 4.5),
                            DockPosition::Left => (px + 4.5, slot.center),
                            DockPosition::Right => (px + pw - 4.5, slot.center),
                        };
                        let is_horiz = matches!(
                            cfg.dock.position,
                            DockPosition::Bottom | DockPosition::Top
                        );
                        let base_rgba = if cfg.appearance.style == DockStyle::CyberNeon {
                            let u = if is_horiz {
                                ((dx - px) / pw.max(1.0)).clamp(0.0, 1.0)
                            } else {
                                ((dy - py) / ph.max(1.0)).clamp(0.0, 1.0)
                            };
                            let c = hsv_to_rgb((rt.neon_phase - u).rem_euclid(1.0), 0.95, 1.0);
                            (c.0, c.1, c.2, 0.98)
                        } else if cfg.appearance.custom_indicator.is_none()
                            && cfg.appearance.adaptive_aura
                        {
                            adaptive_rgb
                                .map(|(r, g, b)| (r, g, b, 0.98))
                                .unwrap_or(pal.dot)
                        } else {
                            pal.dot
                        };
                        cr.set_source_rgba(base_rgba.0, base_rgba.1, base_rgba.2, base_rgba.3);
                        match cfg.dock.indicator_style {
                            IndicatorStyle::Dot => {
                                cr.arc(dx, dy, 2.3, 0.0, PI * 2.0);
                                let _ = cr.fill();
                            }
                            IndicatorStyle::Dash => {
                                if is_horiz {
                                    rounded_rect(cr, dx - 6.0, dy - 1.5, 12.0, 3.0, 1.5);
                                } else {
                                    rounded_rect(cr, dx - 1.5, dy - 6.0, 3.0, 12.0, 1.5);
                                }
                                let _ = cr.fill();
                            }
                            IndicatorStyle::MultiDots => {
                                let count = item.window_ids.len().min(4);
                                let step = 5.4;
                                let span = (count as f64 - 1.0) * step;
                                for k in 0..count {
                                    let off = -span * 0.5 + k as f64 * step;
                                    if is_horiz {
                                        cr.arc(dx + off, dy, 1.9, 0.0, PI * 2.0);
                                    } else {
                                        cr.arc(dx, dy + off, 1.9, 0.0, PI * 2.0);
                                    }
                                    let _ = cr.fill();
                                }
                            }
                            IndicatorStyle::Underline => {
                                let len = (sz * 0.48).clamp(14.0, 28.0);
                                if is_horiz {
                                    rounded_rect(cr, dx - len * 0.5, dy - 1.4, len, 2.8, 1.4);
                                } else {
                                    rounded_rect(cr, dx - 1.4, dy - len * 0.5, 2.8, len, 1.4);
                                }
                                let _ = cr.fill();
                            }
                        }
                    }
                }
            }

            if cfg.dock.autohide_peek && rt.physics.reveal_spring.value < 0.18 {
                let alpha = (1.0 - rt.physics.reveal_spring.value / 0.18).clamp(0.0, 1.0) * 0.68;
                let (sx, sy, sw, sh) = match cfg.dock.position {
                    DockPosition::Bottom => (px + pw * 0.25, h as f64 - 4.0, pw * 0.50, 3.5),
                    DockPosition::Top => (px + pw * 0.25, 0.5, pw * 0.50, 3.5),
                    DockPosition::Left => (0.5, py + ph * 0.25, 3.5, ph * 0.50),
                    DockPosition::Right => (w as f64 - 4.0, py + ph * 0.25, 3.5, ph * 0.50),
                };
                rounded_rect(cr, sx, sy, sw.max(40.0), sh.max(3.5), 2.0);
                cr.set_source_rgba(0.75, 0.86, 1.0, alpha);
                let _ = cr.fill();
            }

            rt.widget_hitboxes.clear();
            let ws_snap = rt.workspaces.clone();
            let media_snap = rt.media.clone();
            let sys_snap = rt.sysinfo;
            let pill_r = rt.pill_rect;
            let ptr_now = rt.pointer;
            let anim_t = rt.hover_time;
            let hitboxes = draw_utility_strip(
                cr,
                &cfg.dock,
                pill_r,
                &ws_snap,
                media_snap.as_ref(),
                sys_snap,
                ptr_now,
                anim_t,
                &mut rt.icon_cache,
                pal,
            );
            rt.widget_hitboxes = hitboxes;

            if let Some(h_idx) = rt.hovered_index {
                if let (Some(item), Some(slot)) = (rt.items.get(h_idx), rt.slots.get(h_idx)) {
                    let bounce_px = rt
                        .physics
                        .items
                        .get(&item.key)
                        .map(|a| a.launch_pose.offset_y + a.hover_lift.value)
                        .unwrap_or(0.0);
                    let tooltip_text = if item.window_ids.len() > 1 {
                        format!("{} ({} windows)", item.label, item.window_ids.len())
                    } else {
                        item.label.clone()
                    };
                    draw_tooltip(
                        cr,
                        &tooltip_text,
                        cfg.dock.position,
                        slot,
                        rt.pill_rect,
                        bounce_px,
                        w as f64,
                        h as f64,
                        pal,
                    );
                }
            }
        });
    }

    fn setup_input(
        self: &Rc<Self>,
        on_open_settings: Rc<dyn Fn()>,
        on_toggle_launchpad: Rc<dyn Fn()>,
    ) {

        let motion = gtk4::EventControllerMotion::new();
        {
            let dock_w = Rc::downgrade(self);
            motion.connect_enter(move |_, x, y| {
                if let Some(dock) = dock_w.upgrade() {
                    let mut rt = dock.runtime.borrow_mut();
                    if let Some(id) = rt.hide_timer.take() {
                        id.remove();
                    }
                    rt.pointer = Some((x, y));
                    rt.physics.hover_spring.target = 1.0;
                    rt.physics.reveal_spring.target = 1.0;
                    rt.tick_active = true;
                }
            });
        }
        {
            let dock_w = Rc::downgrade(self);
            motion.connect_motion(move |_, x, y| {
                if let Some(dock) = dock_w.upgrade() {
                    let mut rt = dock.runtime.borrow_mut();
                    rt.pointer = Some((x, y));
                    rt.physics.hover_spring.target = 1.0;
                    rt.physics.reveal_spring.target = 1.0;
                    rt.tick_active = true;
                }
            });
        }
        {
            let dock_w = Rc::downgrade(self);
            motion.connect_leave(move |_| {
                if let Some(dock) = dock_w.upgrade() {
                    let cfg = dock.state.borrow().config.dock.clone();
                    let mut rt = dock.runtime.borrow_mut();
                    rt.pointer = None;
                    rt.hovered_index = None;
                    rt.physics.hover_spring.target = 0.0;
                    rt.tick_active = true;

                    let should_hide = match cfg.visibility {
                        VisibilityMode::Always => false,
                        VisibilityMode::AutoHide => true,
                        VisibilityMode::Intellihide => dock.check_intellihide_overlap(&rt),
                    };
                    if should_hide {
                        if let Some(id) = rt.hide_timer.take() {
                            id.remove();
                        }
                        let dock_ww = Rc::downgrade(&dock);
                        let id = glib::timeout_add_local_once(
                            Duration::from_millis(cfg.autohide_delay_ms as u64),
                            move || {
                                if let Some(d) = dock_ww.upgrade() {
                                    let mut r = d.runtime.borrow_mut();
                                    r.hide_timer = None;
                                    if r.pointer.is_none() {
                                        r.physics.reveal_spring.target = 0.0;
                                        r.tick_active = true;
                                    }
                                }
                            },
                        );
                        rt.hide_timer = Some(id);
                    }
                }
            });
        }
        self.drawing_area.add_controller(motion);

        let left_click = gtk4::GestureClick::builder().button(1).build();
        {
            let dock_w = Rc::downgrade(self);
            let launchpad_cb = on_toggle_launchpad.clone();
            left_click.connect_released(move |_, _n, x, y| {
                let Some(dock) = dock_w.upgrade() else {
                    return;
                };
                if dock
                    .runtime
                    .borrow()
                    .drag
                    .as_ref()
                    .map(|d| d.active)
                    .unwrap_or(false)
                {
                    return;
                }
                let widget_hit = dock
                    .runtime
                    .borrow()
                    .widget_hitboxes
                    .iter()
                    .find(|hb| hb.contains(x, y) && !matches!(hb.action, WidgetAction::MediaScroll(_)))
                    .map(|hb| hb.action.clone());
                if let Some(action) = widget_hit {
                    match action {
                        WidgetAction::SwitchWorkspace(id) => {
                            dock.state.borrow().tracker.switch_workspace(&id);
                            dock.refresh_widgets();
                            dock.drawing_area.queue_draw();
                        }
                        WidgetAction::MediaPrev(bus) => {
                            mpris_command(&bus, "Previous");
                            dock.refresh_widgets();
                            dock.drawing_area.queue_draw();
                        }
                        WidgetAction::MediaPlayPause(bus) => {
                            mpris_command(&bus, "PlayPause");
                            dock.refresh_widgets();
                            dock.drawing_area.queue_draw();
                        }
                        WidgetAction::MediaNext(bus) => {
                            mpris_command(&bus, "Next");
                            dock.refresh_widgets();
                            dock.drawing_area.queue_draw();
                        }
                        WidgetAction::ShowDesktop => {
                            dock.state.borrow().tracker.show_desktop();
                        }
                        WidgetAction::MediaScroll(_) => {}
                    }
                    return;
                }
                let Some(idx) = dock.hit_test(x, y) else {
                    return;
                };
                let item = dock.runtime.borrow().items[idx].clone();
                match item.kind {
                    DockItemKind::Launchpad => launchpad_cb(),
                    DockItemKind::WindowTile { window_id, .. } => {
                        dock.state.borrow().tracker.focus_window(&window_id);
                    }
                    DockItemKind::Folder { path } => {
                        let dock_pos = dock.state.borrow().config.dock.position;
                        let (px, py, pw, ph) = dock.runtime.borrow().pill_rect;
                        let (ax, ay) = match dock_pos {
                            DockPosition::Bottom => (x, py),
                            DockPosition::Top => (x, py + ph),
                            DockPosition::Left => (px + pw, y),
                            DockPosition::Right => (px, y),
                        };
                        show_folder_stack_popover(&dock.drawing_area, ax, ay, dock_pos, &path);
                    }
                    DockItemKind::Trash => {
                        let _ = crate::trash::TrashDir::open_trash();
                    }
                    DockItemKind::App { desktop_id, .. } => {
                        let (effective_action, focused_wid) = {
                            let st = dock.state.borrow();
                            let eff = if st.config.dock.left_click == LeftClickAction::FocusOrCycle
                                && st.config.dock.click_minimize
                            {
                                LeftClickAction::FocusCycleOrMinimize
                            } else {
                                st.config.dock.left_click
                            };
                            (
                                eff,
                                st.windows.iter().find(|w| w.focused).map(|w| w.id.clone()),
                            )
                        };
                        if !item.window_ids.is_empty()
                            && effective_action == LeftClickAction::FocusOrNewWindow
                        {
                            if !desktop_id.is_empty() {
                                if let Some(entry) =
                                    dock.state.borrow().desktop_index.get(&desktop_id)
                                {
                                    let _ = entry.launch();
                                    return;
                                }
                            }
                        }
                        let last_focused = dock
                            .runtime
                            .borrow()
                            .last_focused_window
                            .get(&item.key)
                            .cloned();
                        match resolve_app_click_action(
                            &item.window_ids,
                            focused_wid.as_deref(),
                            last_focused.as_deref(),
                            effective_action == LeftClickAction::FocusCycleOrMinimize,
                        ) {
                            AppClickAction::Minimize(wid) => {
                                dock.state.borrow().tracker.minimize_window(&wid);
                            }
                            AppClickAction::Focus(wid) => {
                                dock.runtime
                                    .borrow_mut()
                                    .last_focused_window
                                    .insert(item.key.clone(), wid.clone());
                                dock.state.borrow().tracker.focus_window(&wid);
                            }
                            AppClickAction::Launch => {
                                if !desktop_id.is_empty() {
                                    let animate = dock.state.borrow().config.dock.animate_launch;
                                    if animate {
                                        let mut rt = dock.runtime.borrow_mut();
                                        rt.physics.trigger_bounce(&item.key);
                                        rt.tick_active = true;
                                    }
                                    if let Some(entry) =
                                        dock.state.borrow().desktop_index.get(&desktop_id)
                                    {
                                        let _ = entry.launch();
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }
        self.drawing_area.add_controller(left_click);

        let right_click = gtk4::GestureClick::builder().button(3).build();
        {
            let dock_w = Rc::downgrade(self);
            let settings_cb = on_open_settings.clone();
            right_click.connect_pressed(move |_, _n, x, y| {
                let Some(dock) = dock_w.upgrade() else {
                    return;
                };
                let hit = dock.hit_test(x, y);
                let (kind, pinned, wids) = if let Some(idx) = hit {
                    let it = &dock.runtime.borrow().items[idx];
                    (Some(it.kind.clone()), it.pinned, it.window_ids.clone())
                } else {
                    (None, false, Vec::new())
                };
                let dock_refresh = Rc::downgrade(&dock);
                let on_refresh: Rc<dyn Fn()> = Rc::new(move || {
                    if let Some(d) = dock_refresh.upgrade() {
                        d.apply_config();
                    }
                });
                let dock_pos = dock.state.borrow().config.dock.position;
                let (px, py, pw, ph) = dock.runtime.borrow().pill_rect;
                let (ax, ay) = match dock_pos {
                    DockPosition::Bottom => (x, py),
                    DockPosition::Top => (x, py + ph),
                    DockPosition::Left => (px + pw, y),
                    DockPosition::Right => (px, y),
                };
                show_item_menu(
                    &dock.drawing_area,
                    ax,
                    ay,
                    dock_pos,
                    kind,
                    pinned,
                    wids,
                    dock.state.clone(),
                    on_refresh,
                    settings_cb.clone(),
                );
            });
        }
        self.drawing_area.add_controller(right_click);

        let mid_click = gtk4::GestureClick::builder().button(2).build();
        {
            let dock_w = Rc::downgrade(self);
            mid_click.connect_released(move |_, _n, x, y| {
                let Some(dock) = dock_w.upgrade() else {
                    return;
                };
                let action = dock.state.borrow().config.dock.middle_click;
                if action == MiddleClickAction::None {
                    return;
                }
                let Some(idx) = dock.hit_test(x, y) else {
                    return;
                };
                let item = dock.runtime.borrow().items[idx].clone();
                if let DockItemKind::App { desktop_id, .. } = item.kind {
                    match action {
                        MiddleClickAction::NewWindow => {
                            if !desktop_id.is_empty() {
                                if let Some(entry) =
                                    dock.state.borrow().desktop_index.get(&desktop_id)
                                {
                                    let _ = entry.launch();
                                }
                            }
                        }
                        MiddleClickAction::Quit => {
                            let st = dock.state.borrow();
                            for wid in &item.window_ids {
                                st.tracker.close_window(wid);
                            }
                        }
                        MiddleClickAction::None => {}
                    }
                }
            });
        }
        self.drawing_area.add_controller(mid_click);

        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        {
            let dock_w = Rc::downgrade(self);
            scroll.connect_scroll(move |_, _dx, dy| {
                let Some(dock) = dock_w.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if let Some((px, py)) = dock.runtime.borrow().pointer {
                    let hit = dock
                        .runtime
                        .borrow()
                        .widget_hitboxes
                        .iter()
                        .find(|hb| hb.contains(px, py))
                        .map(|hb| hb.action.clone());
                    if let Some(action) = hit {
                        match action {
                            WidgetAction::SwitchWorkspace(_) => {
                                let ws = dock.runtime.borrow().workspaces.clone();
                                if ws.len() > 1 {
                                    let cur = ws.iter().position(|w| w.active).unwrap_or(0);
                                    let next = if dy > 0.0 {
                                        (cur + 1) % ws.len()
                                    } else {
                                        (cur + ws.len() - 1) % ws.len()
                                    };
                                    dock.state.borrow().tracker.switch_workspace(&ws[next].id);
                                    dock.refresh_widgets();
                                    dock.drawing_area.queue_draw();
                                }
                                return glib::Propagation::Stop;
                            }
                            WidgetAction::MediaScroll(bus)
                            | WidgetAction::MediaPrev(bus)
                            | WidgetAction::MediaPlayPause(bus)
                            | WidgetAction::MediaNext(bus) => {
                                let delta = if dy < 0.0 { 0.06 } else { -0.06 };
                                mpris_adjust_volume(&bus, delta);
                                return glib::Propagation::Stop;
                            }
                            WidgetAction::ShowDesktop => {}
                        }
                    }
                }
                if dock.state.borrow().config.dock.scroll_action != ScrollAction::CycleWindows {
                    return glib::Propagation::Proceed;
                }
                let hovered = dock.runtime.borrow().hovered_index;
                let Some(idx) = hovered else {
                    return glib::Propagation::Proceed;
                };
                let item = dock.runtime.borrow().items[idx].clone();
                if item.window_ids.is_empty() {
                    return glib::Propagation::Proceed;
                }
                let focused_wid = dock
                    .state
                    .borrow()
                    .windows
                    .iter()
                    .find(|w| w.focused)
                    .map(|w| w.id.clone());
                let last_focused = dock
                    .runtime
                    .borrow()
                    .last_focused_window
                    .get(&item.key)
                    .cloned();
                if let Some(target_wid) = resolve_scroll_cycle_target(
                    &item.window_ids,
                    focused_wid.as_deref(),
                    last_focused.as_deref(),
                    dy > 0.0,
                ) {
                    dock.runtime
                        .borrow_mut()
                        .last_focused_window
                        .insert(item.key.clone(), target_wid.clone());
                    dock.state.borrow().tracker.focus_window(&target_wid);
                }
                glib::Propagation::Stop
            });
        }
        self.drawing_area.add_controller(scroll);

        let drag = gtk4::GestureDrag::new();
        {
            let dock_w = Rc::downgrade(self);
            drag.connect_drag_begin(move |_, x, y| {
                let Some(dock) = dock_w.upgrade() else {
                    return;
                };
                if let Some(idx) = dock.hit_test(x, y) {
                    let item = dock.runtime.borrow().items[idx].clone();
                    if item.pinned {
                        let drag_id = match item.kind {
                            DockItemKind::App { desktop_id, .. } => Some(desktop_id),
                            DockItemKind::Launchpad => Some("__launchpad__".to_string()),
                            _ => None,
                        };
                        if let Some(desktop_id) = drag_id {
                            dock.runtime.borrow_mut().drag = Some(DragState {
                                item_index: idx,
                                desktop_id,
                                start_x: x,
                                start_y: y,
                                current_x: x,
                                current_y: y,
                                active: false,
                            });
                        }
                    }
                }
            });
        }
        {
            let dock_w = Rc::downgrade(self);
            drag.connect_drag_update(move |_, off_x, off_y| {
                let Some(dock) = dock_w.upgrade() else {
                    return;
                };
                let mut rt = dock.runtime.borrow_mut();
                if let Some(d) = rt.drag.as_mut() {
                    d.current_x = d.start_x + off_x;
                    d.current_y = d.start_y + off_y;
                    if off_x.hypot(off_y) > 8.0 {
                        d.active = true;
                        rt.tick_active = true;
                    }
                }
            });
        }
        {
            let dock_w = Rc::downgrade(self);
            drag.connect_drag_end(move |_, off_x, off_y| {
                let Some(dock) = dock_w.upgrade() else {
                    return;
                };
                let drag_state = dock.runtime.borrow_mut().drag.take();
                let Some(d) = drag_state else {
                    return;
                };
                if !d.active {
                    return;
                }
                let cfg = dock.state.borrow().config.dock.clone();
                let perp = match cfg.position {
                    DockPosition::Bottom | DockPosition::Top => off_y,
                    DockPosition::Left | DockPosition::Right => off_x,
                };

                if d.desktop_id != "__launchpad__" && should_unpin_drag(perp, cfg.icon_size as f64) {
                    dock.state
                        .borrow_mut()
                        .config
                        .apps
                        .pinned
                        .retain(|id| id != &d.desktop_id);
                    AppState::schedule_save(&dock.state);
                    dock.apply_config();
                    return;
                }

                let drop_axis = match cfg.position {
                    DockPosition::Bottom | DockPosition::Top => d.start_x + off_x,
                    DockPosition::Left | DockPosition::Right => d.start_y + off_y,
                };
                let new_pinned_pos = {
                    let rt = dock.runtime.borrow();
                    let mut pinned_centers = Vec::new();
                    for (i, item) in rt.items.iter().enumerate() {
                        if item.pinned && matches!(item.kind, DockItemKind::App { .. }) {
                            if let Some(slot) = rt.slots.get(i) {
                                pinned_centers.push(slot.center);
                            }
                        }
                    }
                    pinned_centers
                        .iter()
                        .filter(|&&c| c < drop_axis)
                        .count()
                };

                {
                    let mut st = dock.state.borrow_mut();
                    if d.desktop_id == "__launchpad__" {
                        st.config.dock.launchpad_index =
                            new_pinned_pos.min(st.config.apps.pinned.len());
                    } else if let Some(old_pos) =
                        st.config.apps.pinned.iter().position(|id| id == &d.desktop_id)
                    {
                        let removed = st.config.apps.pinned.remove(old_pos);
                        let target = new_pinned_pos.min(st.config.apps.pinned.len());
                        st.config.apps.pinned.insert(target, removed);
                    }
                }
                AppState::schedule_save(&dock.state);
                dock.apply_config();
            });
        }
        self.drawing_area.add_controller(drag);

        let drop_target =
            gtk4::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY | gdk::DragAction::MOVE);
        {
            let dock_w = Rc::downgrade(self);
            drop_target.connect_drop(move |_, value, x, y| {
                let Some(dock) = dock_w.upgrade() else {
                    return false;
                };
                let Ok(file_list) = value.get::<gdk::FileList>() else {
                    return false;
                };
                let on_trash = dock
                    .hit_test(x, y)
                    .map(|idx| matches!(dock.runtime.borrow().items[idx].kind, DockItemKind::Trash))
                    .unwrap_or(false);

                let mut changed = false;
                for file in file_list.files() {
                    let Some(path) = file.path() else {
                        continue;
                    };
                    if on_trash {
                        if dock.state.borrow().trash.move_to_trash(&path).is_ok() {
                            changed = true;
                        }
                    } else if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                        if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                            let mut st = dock.state.borrow_mut();
                            if !st.config.apps.pinned.iter().any(|p| p == fname) {
                                st.config.apps.pinned.push(fname.to_string());
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    AppState::schedule_save(&dock.state);
                    dock.apply_config();
                }
                changed
            });
        }
        self.drawing_area.add_controller(drop_target);
    }

    fn hit_test(&self, x: f64, y: f64) -> Option<usize> {
        let cfg = self.state.borrow().config.dock.clone();
        let rt = self.runtime.borrow();
        let (px, py, pw, ph) = rt.pill_rect;
        let in_cross = match cfg.position {
            DockPosition::Bottom => y >= py - cfg.icon_size as f64 && y <= py + ph + 8.0,
            DockPosition::Top => y >= py - 8.0 && y <= py + ph + cfg.icon_size as f64,
            DockPosition::Left => x >= px - 8.0 && x <= px + pw + cfg.icon_size as f64,
            DockPosition::Right => x >= px - cfg.icon_size as f64 && x <= px + pw + 8.0,
        };
        if !in_cross {
            return None;
        }
        let main = match cfg.position {
            DockPosition::Bottom | DockPosition::Top => x,
            DockPosition::Left | DockPosition::Right => y,
        };
        let gap = cfg.icon_spacing as f64;
        rt.slots.iter().enumerate().find_map(|(i, s)| {
            if (main - s.center).abs() <= (s.size + gap) * 0.5 {
                Some(i)
            } else {
                None
            }
        })
    }
}

fn draw_glass_3d_deck(
    cr: &cairo::Context,
    px: f64,
    py: f64,
    pw: f64,
    ph: f64,
    pos: DockPosition,
    pal: crate::ui::GlassPalette,
) {
    let deck_depth = (ph * 0.44).clamp(20.0, 34.0);
    let slant = 18.0;
    let lip = 4.0;

    match pos {
        DockPosition::Bottom => {
            let top_y = py + ph - deck_depth;
            let bot_y = py + ph - lip;

            cr.new_sub_path();
            cr.move_to(px + slant, top_y);
            cr.line_to(px + pw - slant, top_y);
            cr.line_to(px + pw + 4.0, bot_y);
            cr.line_to(px - 4.0, bot_y);
            cr.close_path();

            let deck_grad = cairo::LinearGradient::new(px, top_y, px, bot_y);
            deck_grad.add_color_stop_rgba(0.0, 0.85, 0.88, 0.94, (pal.bg.3 * 0.55).clamp(0.18, 0.58));
            deck_grad.add_color_stop_rgba(0.55, 0.65, 0.69, 0.76, (pal.bg.3 * 0.62).clamp(0.22, 0.66));
            deck_grad.add_color_stop_rgba(1.0, 0.92, 0.94, 0.98, (pal.bg.3 * 0.72).clamp(0.28, 0.75));
            let _ = cr.set_source(&deck_grad);
            let _ = cr.fill_preserve();

            cr.set_source_rgba(1.0, 1.0, 1.0, 0.48);
            cr.set_line_width(1.0);
            let _ = cr.stroke();

            cr.rectangle(px - 4.0, bot_y, pw + 8.0, lip);
            let lip_grad = cairo::LinearGradient::new(px, bot_y, px, bot_y + lip);
            lip_grad.add_color_stop_rgba(0.0, 0.95, 0.97, 1.0, 0.78);
            lip_grad.add_color_stop_rgba(1.0, 0.45, 0.48, 0.55, 0.65);
            let _ = cr.set_source(&lip_grad);
            let _ = cr.fill();
        }
        DockPosition::Top | DockPosition::Left | DockPosition::Right => {

            rounded_rect(cr, px, py, pw, ph, 14.0);
            cr.set_source_rgba(0.75, 0.78, 0.85, (pal.bg.3 * 0.58).clamp(0.20, 0.65));
            let _ = cr.fill_preserve();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.45);
            cr.set_line_width(1.2);
            let _ = cr.stroke();
        }
    }
}

fn draw_cursor_sheen(
    cr: &cairo::Context,
    pill: (f64, f64, f64, f64),
    radius: f64,
    ptr: (f64, f64),
    style: DockStyle,
) {
    let (px, py, pw, ph) = pill;
    let r = match style {
        DockStyle::Capsule => (pw.min(ph) * 0.5).max(16.0),
        DockStyle::Panel => 0.0,
        _ => radius,
    };
    let _ = cr.save();
    rounded_rect(cr, px, py, pw, ph, r);
    cr.clip();

    let sheen_r = pw.max(ph).clamp(120.0, 260.0) * 0.65;
    let grad = cairo::RadialGradient::new(ptr.0, ptr.1, 4.0, ptr.0, ptr.1, sheen_r);
    grad.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.22);
    grad.add_color_stop_rgba(0.45, 1.0, 1.0, 1.0, 0.07);
    grad.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0);
    let _ = cr.set_source(&grad);
    cr.rectangle(px, py, pw, ph);
    let _ = cr.fill();

    let _ = cr.restore();

    if r > 0.5 {
        let _ = cr.save();
        rounded_rect(cr, px + 0.5, py + 0.5, (pw - 1.0).max(1.0), (ph - 1.0).max(1.0), r);
        let rim = cairo::RadialGradient::new(ptr.0, ptr.1, 2.0, ptr.0, ptr.1, 130.0);
        rim.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.55);
        rim.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0);
        let _ = cr.set_source(&rim);
        cr.set_line_width(1.4);
        let _ = cr.stroke();
        let _ = cr.restore();
    }
}

fn draw_window_count_badge(
    cr: &cairo::Context,
    ix: f64,
    iy: f64,
    sz: f64,
    count: usize,
    accent: Option<(f64, f64, f64)>,
) {
    let text = if count > 9 {
        "9+".to_string()
    } else {
        count.to_string()
    };
    let bh = (sz * 0.30).clamp(14.0, 19.0);
    let bw = if text.len() > 1 { bh * 1.35 } else { bh };
    let bx = ix + sz - bw * 0.85;
    let by = iy - bh * 0.12;
    let (r, g, b) = accent.unwrap_or((0.20, 0.62, 0.98));

    rounded_rect(cr, bx, by, bw, bh, bh * 0.5);
    cr.set_source_rgba(r * 0.85, g * 0.85, b * 0.85, 0.96);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.72);
    cr.set_line_width(1.0);
    let _ = cr.stroke();

    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size((bh * 0.64).clamp(9.0, 12.0));
    if let Ok(ext) = cr.text_extents(&text) {
        let tx = bx + (bw - ext.width()) * 0.5 - ext.x_bearing();
        let ty = by + (bh + ext.height()) * 0.5;
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.98);
        cr.move_to(tx, ty);
        let _ = cr.show_text(&text);
    }
}

fn ws_section_length(cfg: &crate::config::DockConfig, ws_count: usize) -> f64 {
    if !cfg.show_workspaces || ws_count == 0 {
        return 0.0;
    }
    let shown = ws_count.min(6) as f64;
    match cfg.position {
        DockPosition::Bottom | DockPosition::Top => shown * 24.0 + 18.0,
        DockPosition::Left | DockPosition::Right => shown * 22.0 + 16.0,
    }
}

fn media_section_length(cfg: &crate::config::DockConfig, has_media: bool) -> f64 {
    if !cfg.show_media || !has_media {
        return 0.0;
    }
    match cfg.position {
        DockPosition::Bottom | DockPosition::Top => 192.0,
        DockPosition::Left | DockPosition::Right => 64.0,
    }
}

fn sysinfo_section_length(cfg: &crate::config::DockConfig) -> f64 {
    if !cfg.show_sysinfo {
        return 0.0;
    }
    match cfg.position {
        DockPosition::Bottom | DockPosition::Top => 86.0,
        DockPosition::Left | DockPosition::Right => 54.0,
    }
}

fn desktop_btn_section_length(cfg: &crate::config::DockConfig) -> f64 {
    if !cfg.show_desktop_btn {
        return 0.0;
    }
    match cfg.position {
        DockPosition::Bottom | DockPosition::Top => 34.0,
        DockPosition::Left | DockPosition::Right => 34.0,
    }
}

fn utility_strip_length(
    cfg: &crate::config::DockConfig,
    ws_count: usize,
    has_media: bool,
) -> f64 {
    ws_section_length(cfg, ws_count)
        + media_section_length(cfg, has_media)
        + sysinfo_section_length(cfg)
        + desktop_btn_section_length(cfg)
        + clock_section_length(cfg)
}

#[allow(clippy::too_many_arguments)]
fn draw_utility_strip(
    cr: &cairo::Context,
    cfg: &crate::config::DockConfig,
    pill: (f64, f64, f64, f64),
    workspaces: &[WorkspaceInfo],
    media: Option<&MediaSnapshot>,
    sysinfo: SysInfoSnapshot,
    ptr: Option<(f64, f64)>,
    anim_time: f64,
    icon_cache: &mut IconCache,
    pal: crate::ui::GlassPalette,
) -> Vec<WidgetHitbox> {
    let mut hitboxes = Vec::new();
    let ws_len = ws_section_length(cfg, workspaces.len());
    let media_len = media_section_length(cfg, media.is_some());
    let sys_len = sysinfo_section_length(cfg);
    let desk_len = desktop_btn_section_length(cfg);
    let clock_len = clock_section_length(cfg);
    let total = ws_len + media_len + sys_len + desk_len + clock_len;
    if total <= 1.0 {
        return hitboxes;
    }

    let (px, py, pw, ph) = pill;
    let is_bottom = matches!(cfg.position, DockPosition::Bottom | DockPosition::Top);
    let mut cursor = if is_bottom {
        px + pw - total - 4.0
    } else {
        py + ph - total - 4.0
    };

    let draw_divider = |cr: &cairo::Context, pos: f64| {
        cr.set_source_rgba(pal.separator.0, pal.separator.1, pal.separator.2, pal.separator.3);
        cr.set_line_width(1.0);
        if is_bottom {
            cr.move_to(pos, py + 12.0);
            cr.line_to(pos, py + ph - 12.0);
        } else {
            cr.move_to(px + 12.0, pos);
            cr.line_to(px + pw - 12.0, pos);
        }
        let _ = cr.stroke();
    };

    if ws_len > 1.0 {
        draw_divider(cr, cursor);
        let shown: Vec<&WorkspaceInfo> = workspaces.iter().take(6).collect();
        let start = cursor + 10.0;
        for (i, ws) in shown.iter().enumerate() {
            let (bx, by, bw, bh) = if is_bottom {
                (start + i as f64 * 24.0, py + (ph - 22.0) * 0.5, 20.0, 22.0)
            } else {
                (px + (pw - 26.0) * 0.5, start + i as f64 * 22.0, 26.0, 18.0)
            };
            let hovered = ptr
                .map(|(mx, my)| mx >= bx && mx <= bx + bw && my >= by && my <= by + bh)
                .unwrap_or(false);
            rounded_rect(cr, bx, by, bw, bh, 6.0);
            if ws.active {
                cr.set_source_rgba(0.24, 0.68, 1.0, 0.92);
            } else if hovered {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.22);
            } else {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.08);
            }
            let _ = cr.fill();

            let label: String = ws.name.chars().take(2).collect();
            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
            cr.set_font_size(11.0);
            if let Ok(ext) = cr.text_extents(&label) {
                let tx = bx + (bw - ext.width()) * 0.5 - ext.x_bearing();
                let ty = by + (bh + ext.height()) * 0.5;
                if ws.active {
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.98);
                } else {
                    cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.78);
                }
                cr.move_to(tx, ty);
                let _ = cr.show_text(&label);
            }
            hitboxes.push(WidgetHitbox {
                x: bx,
                y: by,
                w: bw,
                h: bh,
                action: WidgetAction::SwitchWorkspace(ws.id.clone()),
            });
        }
        cursor += ws_len;
    }

    if let Some(m) = media.filter(|_| media_len > 1.0) {
        draw_divider(cr, cursor);
        if is_bottom {
            let area_x = cursor + 8.0;
            let area_w = media_len - 12.0;
            let art_rgb = m
                .art_path
                .as_deref()
                .and_then(|p| icon_cache.dominant_color(p, 48));
            if let Some((ar, ag, ab)) = art_rgb {
                rounded_rect(cr, area_x - 2.0, py + 7.0, area_w + 4.0, ph - 14.0, 10.0);
                cr.set_source_rgba(ar, ag, ab, 0.22);
                let _ = cr.fill_preserve();
                cr.set_source_rgba(ar, ag, ab, 0.45);
                cr.set_line_width(1.0);
                let _ = cr.stroke();
            }

            hitboxes.push(WidgetHitbox {
                x: area_x,
                y: py + 4.0,
                w: area_w,
                h: ph - 8.0,
                action: WidgetAction::MediaScroll(m.bus_name.clone()),
            });

            let mut text_x = area_x + 4.0;
            let mut text_w = area_w - 22.0;
            if let Some(art_pb) = m
                .art_path
                .as_deref()
                .and_then(|p| icon_cache.get(p, 48))
            {
                let art_sz = (ph - 22.0).clamp(24.0, 34.0);
                let ax = area_x + 4.0;
                let ay = py + (ph - art_sz) * 0.5;
                let _ = cr.save();
                rounded_rect(cr, ax, ay, art_sz, art_sz, 7.0);
                cr.clip();
                cr.translate(ax, ay);
                cr.scale(
                    art_sz / art_pb.width().max(1) as f64,
                    art_sz / art_pb.height().max(1) as f64,
                );
                cr.set_source_pixbuf(&art_pb, 0.0, 0.0);
                let _ = cr.paint();
                let _ = cr.restore();
                text_x = ax + art_sz + 6.0;
                text_w = (area_w - art_sz - 28.0).max(60.0);
            }

            let title = if m.title.chars().count() > 14 {
                let cut: String = m.title.chars().take(13).collect();
                format!("{cut}…")
            } else {
                m.title.clone()
            };
            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
            cr.set_font_size(10.5);
            if let Ok(ext) = cr.text_extents(&title) {
                let tx = text_x + (text_w - ext.width()).max(0.0) * 0.5 - ext.x_bearing();
                let ty = py + ph * 0.37;
                cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.96);
                cr.move_to(tx, ty);
                let _ = cr.show_text(&title);
            }

            let btn_y = py + ph * 0.52;
            let btn_h = 20.0;
            let btn_w = 22.0;
            let cx = text_x + text_w * 0.5;
            let buttons = [
                (cx - 34.0, "‹", WidgetAction::MediaPrev(m.bus_name.clone())),
                (
                    cx - 11.0,
                    if m.playing { "⏸" } else { "▶" },
                    WidgetAction::MediaPlayPause(m.bus_name.clone()),
                ),
                (cx + 12.0, "›", WidgetAction::MediaNext(m.bus_name.clone())),
            ];
            for (bx, glyph, act) in buttons {
                let hovered = ptr
                    .map(|(mx, my)| mx >= bx && mx <= bx + btn_w && my >= btn_y && my <= btn_y + btn_h)
                    .unwrap_or(false);
                if hovered {
                    rounded_rect(cr, bx, btn_y, btn_w, btn_h, 6.0);
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.20);
                    let _ = cr.fill();
                }
                cr.set_font_size(12.0);
                if let Ok(ext) = cr.text_extents(glyph) {
                    let tx = bx + (btn_w - ext.width()) * 0.5 - ext.x_bearing();
                    let ty = btn_y + (btn_h + ext.height()) * 0.5;
                    cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.96);
                    cr.move_to(tx, ty);
                    let _ = cr.show_text(glyph);
                }
                hitboxes.push(WidgetHitbox {
                    x: bx,
                    y: btn_y,
                    w: btn_w,
                    h: btn_h,
                    action: act,
                });
            }

            let eq_x = area_x + area_w - 16.0;
            let eq_base_y = py + ph * 0.72;
            let (er, eg, eb) = art_rgb.unwrap_or((0.32, 0.82, 1.0));
            for bar in 0..4 {
                let h_norm = if m.playing {
                    let freq = 4.2 + bar as f64 * 1.45;
                    let ph_off = bar as f64 * 1.1;
                    0.22 + 0.78 * (0.5 * (1.0 + (anim_time * freq + ph_off).sin()))
                } else {
                    0.20
                };
                let bh = 16.0 * h_norm;
                rounded_rect(cr, eq_x + bar as f64 * 3.8, eq_base_y - bh, 2.5, bh, 1.2);
                cr.set_source_rgba(er, eg, eb, 0.92);
                let _ = cr.fill();
            }
        } else {
            let bx = px + (pw - 28.0) * 0.5;
            let by = cursor + 16.0;
            let glyph = if m.playing { "⏸" } else { "▶" };
            rounded_rect(cr, bx, by, 28.0, 28.0, 8.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.14);
            let _ = cr.fill();
            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
            cr.set_font_size(13.0);
            if let Ok(ext) = cr.text_extents(glyph) {
                cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.95);
                cr.move_to(bx + (28.0 - ext.width()) * 0.5 - ext.x_bearing(), by + (28.0 + ext.height()) * 0.5);
                let _ = cr.show_text(glyph);
            }
            hitboxes.push(WidgetHitbox {
                x: bx,
                y: by,
                w: 28.0,
                h: 28.0,
                action: WidgetAction::MediaPlayPause(m.bus_name.clone()),
            });
        }
        cursor += media_len;
    }

    if sys_len > 1.0 {
        draw_divider(cr, cursor);
        if is_bottom {
            let sx = cursor + 10.0;
            let sw = sys_len - 18.0;
            let cy = py + ph * 0.5;
            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
            cr.set_font_size(10.0);

            if let Some(bat) = sysinfo.battery_pct {
                let label = if sysinfo.battery_charging {
                    format!("⚡ {bat}%")
                } else {
                    format!("BAT {bat}%")
                };
                cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.92);
                cr.move_to(sx, cy - 5.0);
                let _ = cr.show_text(&label);

                let bar_y = cy + 2.0;
                rounded_rect(cr, sx, bar_y, sw, 5.0, 2.5);
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.16);
                let _ = cr.fill();
                let fill_w = (sw * (bat as f64 / 100.0)).max(3.0);
                rounded_rect(cr, sx, bar_y, fill_w, 5.0, 2.5);
                if bat <= 20 && !sysinfo.battery_charging {
                    cr.set_source_rgba(0.98, 0.34, 0.34, 0.95);
                } else {
                    cr.set_source_rgba(0.28, 0.88, 0.54, 0.95);
                }
                let _ = cr.fill();

                cr.set_font_size(9.0);
                cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.72);
                cr.move_to(sx, cy + 18.0);
                let _ = cr.show_text(&format!("C:{}% R:{}%", sysinfo.cpu_pct, sysinfo.mem_pct));
            } else {
                let cpu_label = format!("CPU {}%", sysinfo.cpu_pct);
                cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.88);
                cr.move_to(sx, cy - 8.0);
                let _ = cr.show_text(&cpu_label);
                rounded_rect(cr, sx, cy - 4.0, sw, 4.0, 2.0);
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.16);
                let _ = cr.fill();
                rounded_rect(cr, sx, cy - 4.0, (sw * sysinfo.cpu_pct as f64 / 100.0).max(2.0), 4.0, 2.0);
                cr.set_source_rgba(0.28, 0.78, 1.0, 0.95);
                let _ = cr.fill();

                let ram_label = format!("RAM {}%", sysinfo.mem_pct);
                cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.88);
                cr.move_to(sx, cy + 11.0);
                let _ = cr.show_text(&ram_label);
                rounded_rect(cr, sx, cy + 15.0, sw, 4.0, 2.0);
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.16);
                let _ = cr.fill();
                rounded_rect(cr, sx, cy + 15.0, (sw * sysinfo.mem_pct as f64 / 100.0).max(2.0), 4.0, 2.0);
                cr.set_source_rgba(0.68, 0.52, 1.0, 0.95);
                let _ = cr.fill();
            }
        } else {
            let label = if let Some(bat) = sysinfo.battery_pct {
                format!("{bat}%")
            } else {
                format!("{}%", sysinfo.cpu_pct)
            };
            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
            cr.set_font_size(11.0);
            if let Ok(ext) = cr.text_extents(&label) {
                cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.90);
                cr.move_to(
                    px + (pw - ext.width()) * 0.5 - ext.x_bearing(),
                    cursor + sys_len * 0.5 + ext.height() * 0.5,
                );
                let _ = cr.show_text(&label);
            }
        }
        cursor += sys_len;
    }

    if desk_len > 1.0 {
        draw_divider(cr, cursor);
        let (bx, by, bw, bh) = if is_bottom {
            (cursor + 7.0, py + (ph - 24.0) * 0.5, 22.0, 24.0)
        } else {
            (px + (pw - 24.0) * 0.5, cursor + 7.0, 24.0, 22.0)
        };
        let hovered = ptr
            .map(|(mx, my)| mx >= bx && mx <= bx + bw && my >= by && my <= by + bh)
            .unwrap_or(false);
        rounded_rect(cr, bx, by, bw, bh, 6.0);
        if hovered {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.22);
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.08);
        }
        let _ = cr.fill();
        rounded_rect(cr, bx + 4.5, by + 5.5, bw - 9.0, bh - 12.0, 2.0);
        cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.88);
        cr.set_line_width(1.3);
        let _ = cr.stroke();
        cr.move_to(bx + 7.0, by + bh - 4.0);
        cr.line_to(bx + bw - 7.0, by + bh - 4.0);
        let _ = cr.stroke();
        hitboxes.push(WidgetHitbox {
            x: bx,
            y: by,
            w: bw,
            h: bh,
            action: WidgetAction::ShowDesktop,
        });
    }

    if clock_len > 1.0 {
        draw_embedded_clock(cr, cfg, pill, clock_len, pal);
    }

    hitboxes
}

fn clock_section_length(cfg: &crate::config::DockConfig) -> f64 {
    if !cfg.show_clock {
        return 0.0;
    }
    match cfg.position {
        DockPosition::Bottom | DockPosition::Top => {
            if cfg.clock_show_seconds || !cfg.clock_24h || cfg.clock_show_date {
                108.0
            } else {
                78.0
            }
        }
        DockPosition::Left | DockPosition::Right => {
            if cfg.clock_show_date {
                56.0
            } else {
                38.0
            }
        }
    }
}

fn draw_embedded_clock(
    cr: &cairo::Context,
    cfg: &crate::config::DockConfig,
    pill: (f64, f64, f64, f64),
    clock_len: f64,
    pal: crate::ui::GlassPalette,
) {
    if clock_len <= 1.0 {
        return;
    }
    let now = chrono::Local::now();
    let time_fmt = match (cfg.clock_24h, cfg.clock_show_seconds) {
        (true, false) => "%H:%M",
        (true, true) => "%H:%M:%S",
        (false, false) => "%I:%M %p",
        (false, true) => "%I:%M:%S %p",
    };
    let time_str = now.format(time_fmt).to_string();
    let date_str = if cfg.clock_show_date {
        let fmt = match cfg.clock_date_format {
            ClockDateFormat::DmySlash => "%d/%m/%Y",
            ClockDateFormat::ShortDayMonth => "%a, %b %d",
            ClockDateFormat::MonthDayYear => "%b %d, %Y",
            ClockDateFormat::Iso => "%Y-%m-%d",
        };
        Some(now.format(fmt).to_string())
    } else {
        None
    };

    let (px, py, pw, ph) = pill;
    let (cx, cy) = match cfg.position {
        DockPosition::Bottom | DockPosition::Top => {

            let div_x = px + pw - clock_len - 4.0;
            cr.set_source_rgba(pal.separator.0, pal.separator.1, pal.separator.2, pal.separator.3);
            cr.set_line_width(1.0);
            cr.move_to(div_x, py + 12.0);
            cr.line_to(div_x, py + ph - 12.0);
            let _ = cr.stroke();
            (div_x + (clock_len + 4.0) * 0.5 - 4.0, py + ph * 0.5)
        }
        DockPosition::Left | DockPosition::Right => {
            let div_y = py + ph - clock_len - 4.0;
            cr.set_source_rgba(pal.separator.0, pal.separator.1, pal.separator.2, pal.separator.3);
            cr.set_line_width(1.0);
            cr.move_to(px + 12.0, div_y);
            cr.line_to(px + pw - 12.0, div_y);
            let _ = cr.stroke();
            (px + pw * 0.5, div_y + (clock_len + 4.0) * 0.5 - 2.0)
        }
    };

    let time_size = if cfg.clock_show_seconds || !cfg.clock_24h {
        14.5
    } else {
        17.5
    };
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(time_size);
    if let Ok(ext) = cr.text_extents(&time_str) {
        let tx = cx - ext.width() * 0.5 - ext.x_bearing();
        let ty = if date_str.is_some() {
            cy - 2.0
        } else {
            cy + ext.height() * 0.5
        };
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
        cr.move_to(tx, ty + 1.0);
        let _ = cr.show_text(&time_str);
        cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.96);
        cr.move_to(tx, ty);
        let _ = cr.show_text(&time_str);
    }

    if let Some(date) = date_str {
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(11.0);
        if let Ok(ext) = cr.text_extents(&date) {
            let dx = cx - ext.width() * 0.5 - ext.x_bearing();
            let dy = cy + 14.0;
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.40);
            cr.move_to(dx, dy + 1.0);
            let _ = cr.show_text(&date);
            cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, 0.72);
            cr.move_to(dx, dy);
            let _ = cr.show_text(&date);
        }
    }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let h = ((h % 1.0) + 1.0) % 1.0 * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI * 0.5, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI * 0.5);
    cr.arc(x + r, y + h - r, r, PI * 0.5, PI);
    cr.arc(x + r, y + r, r, PI, PI * 1.5);
    cr.close_path();
}

fn attached_shelf_path(
    cr: &cairo::Context,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    r: f64,
    pos: DockPosition,
) {
    let r = r.min(w * 0.45).min(h * 0.45).max(4.0);
    let flare = (r * 0.42).min(12.0);
    cr.new_sub_path();
    match pos {
        DockPosition::Bottom => {
            cr.move_to(x - flare, y + h);
            cr.curve_to(x, y + h, x, y + h - flare, x, y + h - flare);
            cr.line_to(x, y + r);
            cr.arc(x + r, y + r, r, PI, PI * 1.5);
            cr.line_to(x + w - r, y);
            cr.arc(x + w - r, y + r, r, -PI * 0.5, 0.0);
            cr.line_to(x + w, y + h - flare);
            cr.curve_to(x + w, y + h, x + w + flare, y + h, x + w + flare, y + h);
            cr.close_path();
        }
        DockPosition::Top => {
            cr.move_to(x - flare, y);
            cr.curve_to(x, y, x, y + flare, x, y + flare);
            cr.line_to(x, y + h - r);
            cr.arc(x + r, y + h - r, r, PI, PI * 0.5);
            cr.line_to(x + w - r, y + h);
            cr.arc(x + w - r, y + h - r, r, PI * 0.5, 0.0);
            cr.line_to(x + w, y + flare);
            cr.curve_to(x + w, y, x + w + flare, y, x + w + flare, y);
            cr.close_path();
        }
        DockPosition::Left => {
            cr.move_to(x, y - flare);
            cr.curve_to(x, y, x + flare, y, x + flare, y);
            cr.line_to(x + w - r, y);
            cr.arc(x + w - r, y + r, r, -PI * 0.5, 0.0);
            cr.line_to(x + w, y + h - r);
            cr.arc(x + w - r, y + h - r, r, 0.0, PI * 0.5);
            cr.line_to(x + flare, y + h);
            cr.curve_to(x, y + h, x, y + h + flare, x, y + h + flare);
            cr.close_path();
        }
        DockPosition::Right => {
            cr.move_to(x + w, y - flare);
            cr.curve_to(x + w, y, x + w - flare, y, x + w - flare, y);
            cr.line_to(x + r, y);
            cr.arc(x + r, y + r, r, PI * 1.5, PI);
            cr.line_to(x, y + h - r);
            cr.arc(x + r, y + h - r, r, PI, PI * 0.5);
            cr.line_to(x + w - flare, y + h);
            cr.curve_to(x + w, y + h, x + w, y + h + flare, x + w, y + h + flare);
            cr.close_path();
        }
    }
}

fn draw_window_tile(
    cr: &cairo::Context,
    x: f64,
    y: f64,
    size: f64,
    icon_pb: Option<&gtk4::gdk_pixbuf::Pixbuf>,
    focused: bool,
    pal: crate::ui::GlassPalette,
) {
    let h = size * 0.80;
    let ty = y + (size - h) * 0.5;
    let r = (size * 0.14).max(5.0);

    rounded_rect(cr, x + 1.0, ty, size - 2.0, h, r);
    let grad = cairo::LinearGradient::new(x, ty, x, ty + h);
    grad.add_color_stop_rgba(0.0, 0.22, 0.24, 0.30, 0.94);
    grad.add_color_stop_rgba(1.0, 0.11, 0.12, 0.16, 0.96);
    let _ = cr.set_source(&grad);
    let _ = cr.fill_preserve();

    if focused {
        cr.set_source_rgba(0.22, 0.74, 0.98, 0.96);
        cr.set_line_width(1.8);
    } else {
        cr.set_source_rgba(pal.border_inner.0, pal.border_inner.1, pal.border_inner.2, 0.38);
        cr.set_line_width(1.0);
    }
    let _ = cr.stroke();

    let bar_h = (h * 0.24).clamp(6.0, 12.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.10);
    cr.move_to(x + 2.0, ty + bar_h);
    cr.line_to(x + size - 2.0, ty + bar_h);
    cr.set_line_width(0.8);
    let _ = cr.stroke();

    let dot_r = (size * 0.036).clamp(1.4, 2.6);
    let dot_y = ty + bar_h * 0.5;
    for (idx, (dr, dg, db)) in [
        (1.0, 0.37, 0.34),
        (1.0, 0.74, 0.18),
        (0.16, 0.79, 0.25),
    ]
    .iter()
    .enumerate()
    {
        cr.set_source_rgb(*dr, *dg, *db);
        cr.arc(x + 6.5 + idx as f64 * (dot_r * 2.8), dot_y, dot_r, 0.0, PI * 2.0);
        let _ = cr.fill();
    }

    if let Some(pb) = icon_pb {
        let icon_sz = (h - bar_h) * 0.68;
        let ix = x + (size - icon_sz) * 0.5;
        let iy = ty + bar_h + ((h - bar_h) - icon_sz) * 0.5;
        let _ = cr.save();
        cr.translate(ix, iy);
        let sx = icon_sz / pb.width().max(1) as f64;
        let sy = icon_sz / pb.height().max(1) as f64;
        cr.scale(sx, sy);
        cr.set_source_pixbuf(pb, 0.0, 0.0);
        let _ = cr.paint();
        let _ = cr.restore();
    }
}

fn draw_launchpad_icon(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    let r = size * 0.22;
    rounded_rect(cr, x + 1.0, y + 1.0, size - 2.0, size - 2.0, r);
    let grad = cairo::LinearGradient::new(x, y, x, y + size);
    grad.add_color_stop_rgba(0.0, 0.96, 0.97, 0.99, 0.96);
    grad.add_color_stop_rgba(1.0, 0.78, 0.80, 0.85, 0.96);
    let _ = cr.set_source(&grad);
    let _ = cr.fill();

    let colors = [
        (0.99, 0.36, 0.34),
        (1.00, 0.62, 0.12),
        (0.98, 0.82, 0.16),
        (0.25, 0.80, 0.42),
        (0.18, 0.76, 0.86),
        (0.14, 0.52, 1.00),
        (0.55, 0.38, 0.96),
        (0.94, 0.34, 0.64),
        (0.45, 0.47, 0.54),
    ];
    let pad = size * 0.18;
    let gap = size * 0.08;
    let cell = (size - pad * 2.0 - gap * 2.0) / 3.0;
    for row in 0..3 {
        for col in 0..3 {
            let (cr_c, cg, cb) = colors[row * 3 + col];
            let cx = x + pad + col as f64 * (cell + gap);
            let cy = y + pad + row as f64 * (cell + gap);
            rounded_rect(cr, cx, cy, cell, cell, cell * 0.28);
            cr.set_source_rgb(cr_c, cg, cb);
            let _ = cr.fill();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tooltip(
    cr: &cairo::Context,
    text: &str,
    pos: DockPosition,
    slot: &SlotGeometry,
    pill: (f64, f64, f64, f64),
    bounce_px: f64,
    canvas_w: f64,
    canvas_h: f64,
    pal: crate::ui::GlassPalette,
) {
    let display_text: String = if text.chars().count() > 42 {
        let mut s: String = text.chars().take(41).collect();
        s.push('…');
        s
    } else {
        text.to_string()
    };
    let text = display_text.as_str();
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(12.5);
    let Ok(ext) = cr.text_extents(text) else {
        return;
    };
    let pad_x = 12.0;
    let tw = ext.width() + pad_x * 2.0;
    let th = 26.0;
    let (px, py, pw, ph) = pill;

    let (mut tx, mut ty) = match pos {
        DockPosition::Bottom => (
            slot.center - tw * 0.5,
            py + ph - 10.0 - slot.size - bounce_px - th - 12.0,
        ),
        DockPosition::Top => (
            slot.center - tw * 0.5,
            py + 10.0 + slot.size + bounce_px + 12.0,
        ),
        DockPosition::Left => (
            px + 10.0 + slot.size + bounce_px + 12.0,
            slot.center - th * 0.5,
        ),
        DockPosition::Right => (
            px + pw - 10.0 - slot.size - bounce_px - tw - 12.0,
            slot.center - th * 0.5,
        ),
    };
    tx = tx.clamp(4.0, (canvas_w - tw - 4.0).max(4.0));
    ty = ty.clamp(4.0, (canvas_h - th - 4.0).max(4.0));

    rounded_rect(cr, tx, ty, tw, th, 8.0);
    cr.set_source_rgba(pal.tooltip_bg.0, pal.tooltip_bg.1, pal.tooltip_bg.2, pal.tooltip_bg.3);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(
        pal.tooltip_border.0,
        pal.tooltip_border.1,
        pal.tooltip_border.2,
        pal.tooltip_border.3,
    );
    cr.set_line_width(1.0);
    let _ = cr.stroke();

    if pos == DockPosition::Bottom {
        let tip_x = slot.center.clamp(tx + 10.0, tx + tw - 10.0);
        cr.move_to(tip_x - 5.0, ty + th);
        cr.line_to(tip_x + 5.0, ty + th);
        cr.line_to(tip_x, ty + th + 5.0);
        cr.close_path();
        cr.set_source_rgba(pal.tooltip_bg.0, pal.tooltip_bg.1, pal.tooltip_bg.2, pal.tooltip_bg.3);
        let _ = cr.fill();
    }

    cr.set_source_rgba(pal.tooltip_fg.0, pal.tooltip_fg.1, pal.tooltip_fg.2, pal.tooltip_fg.3);
    cr.move_to(tx + pad_x - ext.x_bearing(), ty + (th + ext.height()) * 0.5 - 1.0);
    let _ = cr.show_text(text);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppClickAction {
    Launch,
    Focus(String),
    Minimize(String),
}

fn resolve_app_click_action(
    window_ids: &[String],
    focused_window_id: Option<&str>,
    last_focused_id: Option<&str>,
    click_minimize: bool,
) -> AppClickAction {
    if window_ids.is_empty() {
        return AppClickAction::Launch;
    }

    if window_ids.len() == 1 {
        let wid = &window_ids[0];
        if click_minimize && focused_window_id == Some(wid.as_str()) {
            return AppClickAction::Minimize(wid.clone());
        }
        return AppClickAction::Focus(wid.clone());
    }

    if let Some(focused) = focused_window_id {
        if let Some(pos) = window_ids.iter().position(|id| id == focused) {
            let next = (pos + 1) % window_ids.len();
            return AppClickAction::Focus(window_ids[next].clone());
        }
    }

    if let Some(last) = last_focused_id {
        if let Some(existing) = window_ids.iter().find(|id| id.as_str() == last) {
            return AppClickAction::Focus(existing.clone());
        }
    }

    AppClickAction::Focus(window_ids[0].clone())
}

fn resolve_scroll_cycle_target(
    window_ids: &[String],
    focused_window_id: Option<&str>,
    last_focused_id: Option<&str>,
    forward: bool,
) -> Option<String> {
    if window_ids.is_empty() {
        return None;
    }
    let len = window_ids.len();
    let cur = focused_window_id
        .and_then(|fw| window_ids.iter().position(|id| id == fw))
        .or_else(|| last_focused_id.and_then(|lw| window_ids.iter().position(|id| id == lw)))
        .unwrap_or(0);
    let next = if forward {
        (cur + 1) % len
    } else {
        (cur + len - 1) % len
    };
    Some(window_ids[next].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_click_cycles_through_two_or_more_windows_when_app_is_focused() {
        let wins = vec!["w1".to_string(), "w2".to_string(), "w3".to_string()];
        assert_eq!(
            resolve_app_click_action(&wins, Some("w1"), None, true),
            AppClickAction::Focus("w2".to_string())
        );
        assert_eq!(
            resolve_app_click_action(&wins, Some("w2"), Some("w2"), true),
            AppClickAction::Focus("w3".to_string())
        );
        assert_eq!(
            resolve_app_click_action(&wins, Some("w3"), Some("w3"), true),
            AppClickAction::Focus("w1".to_string())
        );
    }

    #[test]
    fn app_click_restores_last_focused_window_when_switching_from_another_app() {
        let wins = vec!["w1".to_string(), "w2".to_string()];
        assert_eq!(
            resolve_app_click_action(&wins, Some("other-app-win"), Some("w2"), true),
            AppClickAction::Focus("w2".to_string())
        );
        assert_eq!(
            resolve_app_click_action(&wins, Some("other-app-win"), Some("closed-win"), true),
            AppClickAction::Focus("w1".to_string())
        );
    }

    #[test]
    fn app_click_single_window_focuses_or_minimizes_and_empty_launches() {
        let one = vec!["w1".to_string()];
        assert_eq!(
            resolve_app_click_action(&one, Some("other"), None, true),
            AppClickAction::Focus("w1".to_string())
        );
        assert_eq!(
            resolve_app_click_action(&one, Some("w1"), None, true),
            AppClickAction::Minimize("w1".to_string())
        );
        assert_eq!(
            resolve_app_click_action(&one, Some("w1"), None, false),
            AppClickAction::Focus("w1".to_string())
        );
        assert_eq!(
            resolve_app_click_action(&[], None, None, true),
            AppClickAction::Launch
        );
    }

    #[test]
    fn scroll_cycle_moves_relative_to_focused_window() {
        let wins = vec!["w1".to_string(), "w2".to_string(), "w3".to_string()];
        assert_eq!(
            resolve_scroll_cycle_target(&wins, Some("w2"), None, true),
            Some("w3".to_string())
        );
        assert_eq!(
            resolve_scroll_cycle_target(&wins, Some("w1"), None, false),
            Some("w3".to_string())
        );
    }
}
