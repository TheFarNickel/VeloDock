use crate::config::IntellihideMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn intersects(&self, other: Rect) -> bool {
        if self.w <= 0 || self.h <= 0 || other.w <= 0 || other.h <= 0 {
            return false;
        }
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: String,
    pub app_id: String,
    pub title: String,
    pub focused: bool,
    pub on_active_workspace: bool,
    pub geometry: Option<Rect>,
}

pub trait WindowTracker: Send + Sync {
    fn snapshot(&self) -> Vec<WindowInfo>;
    fn focus_window(&self, id: &str);
    fn close_window(&self, id: &str);
    fn minimize_window(&self, _id: &str) {}
    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        Vec::new()
    }
    fn switch_workspace(&self, _id: &str) {}
    fn show_desktop(&self) {}
    fn start_listener(&self, tx: async_channel::Sender<Vec<WindowInfo>>);
}

pub struct NoopTracker;

impl WindowTracker for NoopTracker {
    fn snapshot(&self) -> Vec<WindowInfo> {
        Vec::new()
    }
    fn focus_window(&self, _id: &str) {}
    fn close_window(&self, _id: &str) {}
    fn start_listener(&self, _tx: async_channel::Sender<Vec<WindowInfo>>) {}
}

pub fn any_window_overlaps(windows: &[WindowInfo], dock_rect: Rect) -> bool {
    windows
        .iter()
        .filter(|w| w.on_active_workspace)
        .filter_map(|w| w.geometry)
        .any(|g| g.intersects(dock_rect))
}

pub fn any_window_overlaps_mode(
    windows: &[WindowInfo],
    dock_rect: Rect,
    monitor_rect: Rect,
    mode: IntellihideMode,
) -> bool {
    windows
        .iter()
        .filter(|w| w.on_active_workspace)
        .filter(|w| match mode {
            IntellihideMode::AllWindows => true,
            IntellihideMode::FocusedOnly => w.focused,
            IntellihideMode::MaximizedOnly => w
                .geometry
                .map(|g| g.w >= (monitor_rect.w * 92) / 100 && g.h >= (monitor_rect.h * 88) / 100)
                .unwrap_or(false),
        })
        .filter_map(|w| w.geometry)
        .any(|g| g.intersects(dock_rect))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intellihide_overlap_detects_intersection() {
        let dock = Rect {
            x: 400,
            y: 1020,
            w: 400,
            h: 60,
        };
        let win = WindowInfo {
            id: "1".into(),
            app_id: "kitty".into(),
            title: String::new(),
            focused: true,
            on_active_workspace: true,
            geometry: Some(Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            }),
        };
        assert!(any_window_overlaps(&[win], dock));

        let other_ws_win = WindowInfo {
            id: "3".into(),
            app_id: "kitty".into(),
            title: String::new(),
            focused: false,
            on_active_workspace: false,
            geometry: Some(Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            }),
        };
        assert!(!any_window_overlaps(&[other_ws_win], dock));

        let non_overlapping = WindowInfo {
            id: "2".into(),
            app_id: "kitty".into(),
            title: String::new(),
            focused: true,
            on_active_workspace: true,
            geometry: Some(Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            }),
        };
        assert!(!any_window_overlaps(&[non_overlapping], dock));
    }
}
