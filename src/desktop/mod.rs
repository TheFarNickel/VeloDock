pub mod entry;

pub use entry::DesktopEntry;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub struct DesktopIndex {
    entries: HashMap<String, DesktopEntry>,
}

impl DesktopIndex {
    pub fn from_entries(list: Vec<DesktopEntry>) -> Self {
        let mut entries = HashMap::new();
        for e in list {
            entries.insert(e.id.clone(), e);
        }
        Self { entries }
    }

    pub fn scan_system() -> Self {
        let mut dirs_to_scan = vec![
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        ];
        if let Some(data_dir) = dirs::data_dir() {
            dirs_to_scan.push(data_dir.join("flatpak/exports/share/applications"));
            dirs_to_scan.push(data_dir.join("applications"));
        }

        let mut entries = HashMap::new();
        for dir in dirs_to_scan {
            let Ok(read_dir) = fs::read_dir(&dir) else {
                continue;
            };
            for item in read_dir.flatten() {
                let path = item.path();
                if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                    continue;
                }
                let Some(id) = path.file_name().and_then(|s| s.to_str()).map(str::to_string) else {
                    continue;
                };
                let Ok(content) = fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(parsed) = DesktopEntry::parse_str(&id, path, &content) {

                    entries.insert(id, parsed);
                }
            }
        }
        Self { entries }
    }

    pub fn get(&self, id: &str) -> Option<&DesktopEntry> {
        self.entries.get(id)
    }

    pub fn all_ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn visible_sorted(&self) -> Vec<&DesktopEntry> {
        let mut list: Vec<&DesktopEntry> = self
            .entries
            .values()
            .filter(|e| !e.no_display && !e.hidden)
            .collect();
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        list
    }

    pub fn match_app_id(&self, app_id: &str) -> Option<&DesktopEntry> {
        let trimmed = app_id.trim();
        if trimmed.is_empty() {
            return None;
        }
        let base_name = std::path::Path::new(trimmed)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(trimmed);
        let needle = base_name.to_lowercase();
        if needle.is_empty() {
            return None;
        }
        let needle_stem = needle.trim_end_matches(".desktop");
        let needle_with_ext = format!("{needle_stem}.desktop");
        let needle_last = needle_stem.rsplit('.').next().unwrap_or(needle_stem);

        let find_preferred = |predicate: &dyn Fn(&DesktopEntry) -> bool| -> Option<&DesktopEntry> {
            let mut best_visible: Option<&DesktopEntry> = None;
            let mut best_hidden: Option<&DesktopEntry> = None;
            for entry in self.entries.values() {
                if !predicate(entry) {
                    continue;
                }
                let is_visible = !entry.no_display && !entry.hidden;
                let slot = if is_visible {
                    &mut best_visible
                } else {
                    &mut best_hidden
                };
                if slot.map(|cur| entry.id < cur.id).unwrap_or(true) {
                    *slot = Some(entry);
                }
            }
            best_visible.or(best_hidden)
        };

        if let Some(found) = find_preferred(&|e| e.id.eq_ignore_ascii_case(&needle_with_ext)) {
            return Some(found);
        }

        if let Some(found) = find_preferred(&|e| {
            e.startup_wm_class
                .as_deref()
                .map(|c| {
                    c.eq_ignore_ascii_case(&needle) || c.eq_ignore_ascii_case(needle_stem)
                })
                .unwrap_or(false)
        }) {
            return Some(found);
        }

        if let Some(found) = find_preferred(&|e| {
            let stem = e.id.trim_end_matches(".desktop").to_lowercase();
            let entry_last = stem.rsplit('.').next().unwrap_or(&stem);
            entry_last == needle_stem || stem == needle_last || entry_last == needle_last
        }) {
            return Some(found);
        }

        find_preferred(&|e| {
            let bin = e.exec_binary_name();
            bin.as_deref() == Some(needle_stem) || bin.as_deref() == Some(needle_last)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, name: &str, exec: &str, wm_class: Option<&str>, no_display: bool) -> DesktopEntry {
        DesktopEntry {
            id: id.to_string(),
            name: name.to_string(),
            exec: exec.to_string(),
            icon: None,
            startup_wm_class: wm_class.map(str::to_string),
            no_display,
            hidden: false,
            terminal: false,
            categories: Vec::new(),
            path: PathBuf::new(),
        }
    }

    #[test]
    fn prefers_visible_desktop_entry_over_nodisplay_handler() {
        let idx = DesktopIndex::from_entries(vec![
            make_entry("code-url-handler.desktop", "VS Code URL", "/usr/bin/code %U", Some("Code"), true),
            make_entry("code.desktop", "VS Code", "/usr/bin/code %F", Some("Code"), false),
        ]);
        assert_eq!(idx.match_app_id("code").unwrap().id, "code.desktop");
        assert_eq!(idx.match_app_id("Code").unwrap().id, "code.desktop");
    }

    #[test]
    fn matches_kde_reverse_dns_paths_and_flatpak_ids() {
        let idx = DesktopIndex::from_entries(vec![
            make_entry("org.kde.dolphin.desktop", "Dolphin", "dolphin %u", None, false),
            make_entry("firefox.desktop", "Firefox", "/usr/lib/firefox/firefox %u", None, false),
        ]);
        assert_eq!(
            idx.match_app_id("/usr/share/applications/org.kde.dolphin.desktop").unwrap().id,
            "org.kde.dolphin.desktop"
        );
        assert_eq!(idx.match_app_id("org.kde.dolphin").unwrap().id, "org.kde.dolphin.desktop");
        assert_eq!(idx.match_app_id("dolphin").unwrap().id, "org.kde.dolphin.desktop");
        assert_eq!(idx.match_app_id("org.mozilla.firefox").unwrap().id, "firefox.desktop");
    }
}
