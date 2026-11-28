pub mod hyprland;
pub mod kde;
pub mod niri;
pub mod surface;
pub mod sway;
pub mod tracker;
pub mod x11;

pub use tracker::{
    any_window_overlaps, any_window_overlaps_mode, NoopTracker, Rect, WindowInfo, WindowTracker,
    WorkspaceInfo,
};

pub fn is_hyprland() -> bool {
    std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .ok()
        .filter(|s| !s.is_empty())
        .is_some()
}

pub fn create_window_tracker() -> Box<dyn WindowTracker> {
    if let Some(hypr) = hyprland::HyprlandTracker::from_env() {
        return Box::new(hypr);
    }
    if let Some(niri) = niri::NiriTracker::from_env() {
        return Box::new(niri);
    }
    if let Some(sway) = sway::SwayTracker::from_env() {
        return Box::new(sway);
    }
    if let Some(kde_wl) = kde::KdeWaylandTracker::from_env() {
        return Box::new(kde_wl);
    }
    if x11::X11Tracker::is_available() {
        return Box::new(x11::X11Tracker);
    }
    Box::new(NoopTracker)
}
