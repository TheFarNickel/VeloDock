pub mod defaults;

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockPosition {
    Bottom,
    Top,
    Left,
    Right,
}

impl Default for DockPosition {
    fn default() -> Self {
        Self::Bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisibilityMode {
    Always,
    AutoHide,
    Intellihide,
}

impl Default for VisibilityMode {
    fn default() -> Self {
        Self::Always
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Auto,
    Light,
    Dark,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockStyle {
    #[serde(alias = "sunsetglow", alias = "mattetile")]
    Floating,
    #[serde(alias = "edgenotch")]
    Attached,
    Glass3D,
    Panel,
    SplitIslands,
    BaselineBar,
    Aurora,
    EdgeRail,
    CyberNeon,
    Capsule,
    Invisible,
}

impl Default for DockStyle {
    fn default() -> Self {
        Self::Floating
    }
}

impl DockStyle {
    pub const ALL: &'static [DockStyle] = &[
        DockStyle::Floating,
        DockStyle::Attached,
        DockStyle::Glass3D,
        DockStyle::Panel,
        DockStyle::SplitIslands,
        DockStyle::BaselineBar,
        DockStyle::Aurora,
        DockStyle::EdgeRail,
        DockStyle::CyberNeon,
        DockStyle::Capsule,
        DockStyle::Invisible,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            DockStyle::Floating => "Floating Glass",
            DockStyle::Attached => "Attached Shelf",
            DockStyle::Glass3D => "3D Glass Deck",
            DockStyle::Panel => "Full Panel",
            DockStyle::SplitIslands => "Split Islands",
            DockStyle::BaselineBar => "Baseline Bar",
            DockStyle::Aurora => "Aurora Glass",
            DockStyle::EdgeRail => "Edge Rail",
            DockStyle::CyberNeon => "Cyber Neon",
            DockStyle::Capsule => "Dynamic Capsule",
            DockStyle::Invisible => "Floating Minimal",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            DockStyle::Floating => "Classic frosted glass pill",
            DockStyle::Attached => "Flush edge shelf with flared base",
            DockStyle::Glass3D => "Retro 3D perspective glass tray",
            DockStyle::Panel => "Edge-to-edge translucent bar",
            DockStyle::SplitIslands => "Separate frosted glass islands per section",
            DockStyle::BaselineBar => "Ultra-clean floating glass rail beneath icons",
            DockStyle::Aurora => "Subtle shifting aurora tint over frosted glass",
            DockStyle::EdgeRail => "Sleek full-edge workstation rail with fine inner gleam",
            DockStyle::CyberNeon => "Obsidian glass with flowing neon rim",
            DockStyle::Capsule => "Ultra-rounded pill island look",
            DockStyle::Invisible => "Pure floating icons, no tray",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MiddleClickAction {
    NewWindow,
    Quit,
    None,
}

impl Default for MiddleClickAction {
    fn default() -> Self {
        Self::NewWindow
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollAction {
    CycleWindows,
    None,
}

impl Default for ScrollAction {
    fn default() -> Self {
        Self::CycleWindows
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntellihideMode {
    AllWindows,
    FocusedOnly,
    MaximizedOnly,
}

impl Default for IntellihideMode {
    fn default() -> Self {
        Self::AllWindows
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeftClickAction {
    FocusOrCycle,
    FocusCycleOrMinimize,
    FocusOrNewWindow,
}

impl Default for LeftClickAction {
    fn default() -> Self {
        Self::FocusOrCycle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndicatorStyle {
    Dot,
    Dash,
    MultiDots,
    Underline,
}

impl Default for IndicatorStyle {
    fn default() -> Self {
        Self::Dot
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HoverAnimation {
    Wave,
    Spotlight,
    #[serde(alias = "domino")]
    CascadeLean,
    #[serde(alias = "cylinder3d")]
    OrbitalArc,
    #[serde(alias = "coverflow3d", alias = "cardfan", alias = "cascade")]
    PrismDeck,
    #[serde(alias = "jellysquash")]
    RubberFlex,
    #[serde(alias = "magneticparting", alias = "singlefocus")]
    MagnetSplit,
    #[serde(alias = "pendulum")]
    InertiaSway,
    #[serde(alias = "liquidripple", alias = "floatlift")]
    PulseWave,
}

impl Default for HoverAnimation {
    fn default() -> Self {
        Self::Wave
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchAnimation {
    Bounce,
    ElasticPop,
    PulseGlow,
    Wiggle,
    Spin360,
    #[serde(alias = "coinflip3d")]
    AxisFlip,
    #[serde(alias = "heartbeat")]
    DoublePulse,
    #[serde(alias = "squish")]
    SpringPress,
    #[serde(alias = "tada")]
    PopCelebrate,
    #[serde(alias = "swing")]
    DampedSway,
    #[serde(alias = "shake")]
    QuickJolt,
    #[serde(alias = "roll")]
    ArcRoll,
}

impl Default for LaunchAnimation {
    fn default() -> Self {
        Self::Bounce
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClockDateFormat {
    DmySlash,
    ShortDayMonth,
    MonthDayYear,
    Iso,
}

impl Default for ClockDateFormat {
    fn default() -> Self {
        Self::DmySlash
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockConfig {
    pub icon_size: u32,
    pub icon_spacing: u32,
    pub magnification: bool,
    pub magnification_scale: f64,
    pub magnification_spread: f64,
    pub hover_lift: f64,
    pub hover_animation: HoverAnimation,
    pub animation_smoothness: f64,
    pub bounce_intensity: f64,
    pub launch_animation: LaunchAnimation,
    pub position: DockPosition,
    pub visibility: VisibilityMode,
    pub intellihide_mode: IntellihideMode,
    pub autohide_delay_ms: u32,
    pub autohide_peek: bool,
    pub show_indicators: bool,
    pub indicator_style: IndicatorStyle,
    pub show_window_badges: bool,
    pub left_click: LeftClickAction,
    pub click_minimize: bool,
    pub show_unpinned: bool,
    pub isolate_workspaces: bool,
    pub show_window_tiles: bool,
    pub animate_launch: bool,
    pub show_launchpad: bool,
    pub launchpad_index: usize,
    pub show_trash: bool,
    pub show_workspaces: bool,
    pub show_media: bool,
    pub show_sysinfo: bool,
    pub show_desktop_btn: bool,
    pub show_clock: bool,
    pub clock_show_date: bool,
    pub clock_show_seconds: bool,
    pub clock_24h: bool,
    pub clock_date_format: ClockDateFormat,
    pub monitor_name: Option<String>,
    pub middle_click: MiddleClickAction,
    pub scroll_action: ScrollAction,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            icon_size: 52,
            icon_spacing: 8,
            magnification: true,
            magnification_scale: 1.65,
            magnification_spread: 2.6,
            hover_lift: 0.0,
            hover_animation: HoverAnimation::Wave,
            animation_smoothness: 1.0,
            bounce_intensity: 1.0,
            launch_animation: LaunchAnimation::Bounce,
            position: DockPosition::Bottom,
            visibility: VisibilityMode::Always,
            intellihide_mode: IntellihideMode::AllWindows,
            autohide_delay_ms: 180,
            autohide_peek: false,
            show_indicators: true,
            indicator_style: IndicatorStyle::Dot,
            show_window_badges: false,
            left_click: LeftClickAction::FocusOrCycle,
            click_minimize: false,
            show_unpinned: true,
            isolate_workspaces: false,
            show_window_tiles: false,
            animate_launch: true,
            show_launchpad: true,
            launchpad_index: 0,
            show_trash: true,
            show_workspaces: false,
            show_media: false,
            show_sysinfo: false,
            show_desktop_btn: false,
            show_clock: false,
            clock_show_date: true,
            clock_show_seconds: false,
            clock_24h: true,
            clock_date_format: ClockDateFormat::DmySlash,
            monitor_name: None,
            middle_click: MiddleClickAction::NewWindow,
            scroll_action: ScrollAction::CycleWindows,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub style: DockStyle,
    pub theme: ThemeMode,
    pub opacity: f64,
    pub dynamic_opacity: bool,
    pub pywal_theme: bool,
    pub corner_radius: u32,
    pub edge_margin: u32,
    pub show_separators: bool,
    pub hover_highlight: bool,
    pub cursor_sheen: bool,
    pub adaptive_aura: bool,
    pub icon_shadow: bool,
    pub border_width: f64,
    pub neon_speed: f64,
    pub custom_tint: Option<String>,
    pub custom_indicator: Option<String>,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            style: DockStyle::Floating,
            theme: ThemeMode::Auto,
            opacity: 0.72,
            dynamic_opacity: false,
            pywal_theme: false,
            corner_radius: 20,
            edge_margin: 6,
            show_separators: true,
            hover_highlight: false,
            cursor_sheen: false,
            adaptive_aura: false,
            icon_shadow: false,
            border_width: 1.0,
            neon_speed: 1.15,
            custom_tint: None,
            custom_indicator: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppsConfig {
    pub pinned: Vec<String>,
    pub pinned_folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchpadConfig {
    pub columns: u32,
    pub rows: u32,
    pub icon_size: u32,
    pub backdrop_opacity: f64,
    pub tile_hover_scale: f64,
    pub paged_mode: bool,
    pub show_categories: bool,
    pub show_labels: bool,
    pub custom_bg: Option<String>,
}

impl Default for LaunchpadConfig {
    fn default() -> Self {
        Self {
            columns: 7,
            rows: 5,
            icon_size: 64,
            backdrop_opacity: 0.78,
            tile_hover_scale: 1.10,
            paged_mode: true,
            show_categories: true,
            show_labels: true,
            custom_bg: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub dock: DockConfig,
    pub appearance: AppearanceConfig,
    pub apps: AppsConfig,
    pub launchpad: LaunchpadConfig,
}

impl Config {
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("velodock")
            .join("config.toml")
    }

    pub fn clamp(&mut self) {
        self.dock.icon_size = self.dock.icon_size.clamp(32, 80);
        self.dock.icon_spacing = self.dock.icon_spacing.clamp(2, 24);
        self.dock.magnification_scale = if self.dock.magnification_scale.is_finite() {
            self.dock.magnification_scale.clamp(1.0, 2.2)
        } else {
            1.65
        };
        self.dock.magnification_spread = if self.dock.magnification_spread.is_finite() {
            self.dock.magnification_spread.clamp(1.6, 4.0)
        } else {
            2.6
        };
        self.dock.hover_lift = if self.dock.hover_lift.is_finite() {
            self.dock.hover_lift.clamp(0.0, 48.0)
        } else {
            0.0
        };
        self.dock.animation_smoothness = if self.dock.animation_smoothness.is_finite() {
            self.dock.animation_smoothness.clamp(0.3, 2.0)
        } else {
            1.0
        };
        self.dock.bounce_intensity = if self.dock.bounce_intensity.is_finite() {
            self.dock.bounce_intensity.clamp(0.4, 1.8)
        } else {
            1.0
        };
        self.dock.autohide_delay_ms = self.dock.autohide_delay_ms.clamp(0, 1000);
        self.appearance.opacity = if self.appearance.opacity.is_finite() {
            self.appearance.opacity.clamp(0.0, 1.0)
        } else {
            0.72
        };
        self.appearance.corner_radius = self.appearance.corner_radius.clamp(0, 32);
        self.appearance.edge_margin = self.appearance.edge_margin.clamp(0, 24);
        self.appearance.border_width = if self.appearance.border_width.is_finite() {
            self.appearance.border_width.clamp(0.0, 3.5)
        } else {
            1.0
        };
        self.appearance.neon_speed = if self.appearance.neon_speed.is_finite() {
            self.appearance.neon_speed.clamp(0.2, 3.5)
        } else {
            1.15
        };
        self.launchpad.columns = self.launchpad.columns.clamp(4, 10);
        self.launchpad.rows = self.launchpad.rows.clamp(3, 7);
        self.launchpad.icon_size = self.launchpad.icon_size.clamp(44, 96);
        self.launchpad.backdrop_opacity = if self.launchpad.backdrop_opacity.is_finite() {
            self.launchpad.backdrop_opacity.clamp(0.15, 0.96)
        } else {
            0.78
        };
        self.launchpad.tile_hover_scale = if self.launchpad.tile_hover_scale.is_finite() {
            self.launchpad.tile_hover_scale.clamp(1.0, 1.24)
        } else {
            1.10
        };
    }

    pub fn load_from_path(path: &Path) -> (Self, bool) {
        if !path.exists() {
            return (Self::default(), true);
        }
        match fs::read_to_string(path) {
            Ok(raw) => match toml::from_str::<Config>(&raw) {
                Ok(mut cfg) => {
                    cfg.clamp();
                    (cfg, false)
                }
                Err(err) => {
                    eprintln!("archdock: invalid config at {}: {err}; backing up", path.display());
                    let bak = path.with_extension("toml.bak");
                    let _ = fs::rename(path, bak);
                    (Self::default(), true)
                }
            },
            Err(err) => {
                eprintln!("archdock: failed reading {}: {err}", path.display());
                (Self::default(), true)
            }
        }
    }

    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut clamped = self.clone();
        clamped.clamp();
        let serialized = toml::to_string_pretty(&clamped)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, serialized)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn parse_hex_rgb(hex: &str) -> Option<(f64, f64, f64)> {
    let s = hex.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()? as f64 / 255.0;
    let g = u8::from_str_radix(&s[2..4], 16).ok()? as f64 / 255.0;
    let b = u8::from_str_radix(&s[4..6], 16).ok()? as f64 / 255.0;
    Some((r, g, b))
}

pub fn load_pywal_colors() -> Option<((f64, f64, f64), (f64, f64, f64))> {
    let wal_path = dirs::cache_dir()?.join("wal").join("colors.json");
    let raw = fs::read_to_string(wal_path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let bg_hex = val.get("special")?.get("background")?.as_str()?;
    let accent_hex = val
        .get("colors")?
        .get("color4")
        .or_else(|| val.get("colors")?.get("color6"))
        .and_then(|v| v.as_str())?;
    Some((parse_hex_rgb(bg_hex)?, parse_hex_rgb(accent_hex)?))
}

pub fn autostart_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("autostart")
}

pub fn is_autostart_enabled_in(dir: &Path) -> bool {
    dir.join("velodock.desktop").exists()
}

pub fn is_autostart_enabled() -> bool {
    is_autostart_enabled_in(&autostart_dir())
}

pub fn set_autostart_in(dir: &Path, enabled: bool) -> io::Result<()> {
    let entry_path = dir.join("velodock.desktop");
    if enabled {
        fs::create_dir_all(dir)?;
        let exec = std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(str::to_string))
            .filter(|p| p.starts_with("/usr/"))
            .unwrap_or_else(|| "velodock".to_string());
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=VeloDock\nComment=Ultra-smooth desktop Dock for Arch Linux\nExec={exec} --dock-only\nIcon=velodock\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
        );
        fs::write(&entry_path, content)?;
    } else if entry_path.exists() {
        fs::remove_file(&entry_path)?;
    }
    Ok(())
}

pub const HYPRLAND_LUA_AUTOSTART_BODY: &str = "hl.on(\"hyprland.start\", function()\n  hl.exec_cmd(\"velodock --dock-only\")\nend)";
pub const HYPRLAND_LUA_BLUR_BODY: &str = "hl.layer_rule({\n  name = \"velodock-blur\",\n  match = { namespace = \"^velodock$\" },\n  blur = true,\n  ignore_alpha = 0,\n})\nhl.layer_rule({\n  name = \"velodock-launchpad-blur\",\n  match = { namespace = \"^velodock-launchpad$\" },\n  blur = true,\n})";
pub const HYPRLAND_CONF_BLUR_BODY: &str = "layerrule = blur, velodock\nlayerrule = ignorezero, velodock\nlayerrule = blur, velodock-launchpad";

pub fn set_autostart(enabled: bool) -> io::Result<()> {
    set_autostart_in(&autostart_dir(), enabled)?;

    if let Some(hypr_lua) = hyprland_lua_config_path().filter(|p| p.exists()) {
        let _ = set_managed_block_with_prefix(
            &hypr_lua,
            "--",
            "velodock autostart",
            if enabled {
                Some(HYPRLAND_LUA_AUTOSTART_BODY)
            } else {
                None
            },
        );
        if let Some(hypr_conf) = hyprland_config_path().filter(|p| p.exists()) {
            if has_managed_block(&hypr_conf, "velodock autostart") {
                let _ = set_managed_block(&hypr_conf, "velodock autostart", None);
            }
        }
    } else if let Some(hypr_conf) = hyprland_config_path().filter(|p| p.exists()) {
        let _ = set_managed_block(
            &hypr_conf,
            "velodock autostart",
            if enabled {
                Some("exec-once = velodock --dock-only")
            } else {
                None
            },
        );
    }
    if let Some(sway_conf) = sway_config_path().filter(|p| p.exists()) {
        let _ = set_managed_block(
            &sway_conf,
            "velodock autostart",
            if enabled {
                Some("exec velodock --dock-only")
            } else {
                None
            },
        );
    }
    Ok(())
}

pub fn hyprland_lua_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("hypr").join("hyprland.lua"))
}

pub fn hyprland_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("hypr").join("hyprland.conf"))
}

pub fn is_hyprland_lua_active() -> bool {
    hyprland_lua_config_path().map(|p| p.exists()).unwrap_or(false)
}

pub fn sway_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("sway").join("config"))
}

pub fn is_hyprland_blur_enabled() -> bool {
    if let Some(hypr_lua) = hyprland_lua_config_path().filter(|p| p.exists()) {
        return has_managed_block_with_prefix(&hypr_lua, "--", "velodock blur");
    }
    let Some(path) = hyprland_config_path().filter(|p| p.exists()) else {
        return false;
    };
    has_managed_block(&path, "velodock blur")
}

pub fn set_hyprland_blur(enabled: bool) -> io::Result<()> {
    if let Some(hypr_lua) = hyprland_lua_config_path().filter(|p| p.exists()) {
        let body = if enabled {
            Some(HYPRLAND_LUA_BLUR_BODY)
        } else {
            None
        };
        set_managed_block_with_prefix(&hypr_lua, "--", "velodock blur", body)?;
        if let Some(hypr_conf) = hyprland_config_path().filter(|p| p.exists()) {
            if has_managed_block(&hypr_conf, "velodock blur") {
                let _ = set_managed_block(&hypr_conf, "velodock blur", None);
            }
        }
    } else if let Some(path) = hyprland_config_path().filter(|p| p.exists()) {
        let body = if enabled {
            Some(HYPRLAND_CONF_BLUR_BODY)
        } else {
            None
        };
        set_managed_block(&path, "velodock blur", body)?;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "~/.config/hypr/hyprland.lua or hyprland.conf not found",
        ));
    }

    let _ = std::process::Command::new("hyprctl")
        .arg("reload")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    Ok(())
}

pub fn has_managed_block(path: &Path, tag: &str) -> bool {
    has_managed_block_with_prefix(path, "#", tag)
}

pub fn has_managed_block_with_prefix(path: &Path, comment_prefix: &str, tag: &str) -> bool {
    let start = format!("{comment_prefix} >>> {tag} >>>");
    fs::read_to_string(path)
        .map(|raw| raw.contains(&start))
        .unwrap_or(false)
}

pub fn set_managed_block(path: &Path, tag: &str, body: Option<&str>) -> io::Result<()> {
    set_managed_block_with_prefix(path, "#", tag, body)
}

pub fn set_managed_block_with_prefix(
    path: &Path,
    comment_prefix: &str,
    tag: &str,
    body: Option<&str>,
) -> io::Result<()> {
    let start_marker = format!("{comment_prefix} >>> {tag} >>>");
    let end_marker = format!("{comment_prefix} <<< {tag} <<<");

    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut kept_lines: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in existing.lines() {
        if line.trim() == start_marker {
            skipping = true;
            continue;
        }
        if skipping {
            if line.trim() == end_marker {
                skipping = false;
            }
            continue;
        }
        kept_lines.push(line);
    }

    while kept_lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        kept_lines.pop();
    }

    let mut output = kept_lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }

    if let Some(content) = body {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&start_marker);
        output.push('\n');
        output.push_str(content.trim());
        output.push('\n');
        output.push_str(&end_marker);
        output.push('\n');
    }

    let tmp = path.with_extension("velodock.tmp");
    fs::write(&tmp, output)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn roundtrip_and_clamp() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.dock.icon_size = 999;
        cfg.dock.magnification_scale = 0.2;
        cfg.appearance.opacity = -1.0;
        cfg.clamp();
        assert_eq!(cfg.dock.icon_size, 80);
        assert!((cfg.dock.magnification_scale - 1.0).abs() < 1e-6);
        assert!((cfg.appearance.opacity - 0.0).abs() < 1e-6);
        cfg.save_to_path(&path).unwrap();
        let (loaded, created) = Config::load_from_path(&path);
        assert!(!created);
        assert_eq!(loaded.dock.icon_size, 80);
    }

    #[test]
    fn corrupted_config_backs_up_and_recovers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "not valid = [toml").unwrap();
        let (loaded, _created) = Config::load_from_path(&path);
        assert_eq!(loaded.dock.icon_size, 52);
        assert!(dir.path().join("config.toml.bak").exists());
    }

    #[test]
    fn detect_default_pinned_picks_available() {
        let avail = vec!["kitty.desktop".into(), "firefox.desktop".into()];
        let pinned = defaults::detect_default_pinned(&avail);
        assert_eq!(pinned, vec!["firefox.desktop", "kitty.desktop"]);
    }

    #[test]
    fn autostart_creates_dock_only_entry_and_removes() {
        let dir = tempdir().unwrap();
        assert!(!is_autostart_enabled_in(dir.path()));
        set_autostart_in(dir.path(), true).unwrap();
        assert!(is_autostart_enabled_in(dir.path()));
        let content = fs::read_to_string(dir.path().join("velodock.desktop")).unwrap();
        assert!(content.contains("--dock-only"));
        set_autostart_in(dir.path(), false).unwrap();
        assert!(!is_autostart_enabled_in(dir.path()));
    }

    #[test]
    fn parses_hex_rgb_colors() {
        let (r, g, b) = parse_hex_rgb("#38bdf8").unwrap();
        assert!((r - 0x38 as f64 / 255.0).abs() < 1e-6);
        assert!((g - 0xbd as f64 / 255.0).abs() < 1e-6);
        assert!((b - 0xf8 as f64 / 255.0).abs() < 1e-6);
        assert!(parse_hex_rgb("invalid").is_none());
    }

    #[test]
    fn managed_block_adds_updates_and_removes_cleanly() {
        let dir = tempdir().unwrap();
        let conf = dir.path().join("hyprland.conf");
        fs::write(&conf, "monitor=,preferred,auto,1\n").unwrap();

        set_managed_block(&conf, "velodock blur", Some("layerrule = blur, velodock")).unwrap();
        assert!(has_managed_block(&conf, "velodock blur"));
        let raw = fs::read_to_string(&conf).unwrap();
        assert!(raw.contains("monitor=,preferred,auto,1"));
        assert!(raw.contains("layerrule = blur, velodock"));

        set_managed_block(&conf, "velodock blur", None).unwrap();
        assert!(!has_managed_block(&conf, "velodock blur"));
        let after = fs::read_to_string(&conf).unwrap();
        assert_eq!(after, "monitor=,preferred,auto,1\n");
    }

    #[test]
    fn managed_block_lua_prefix_adds_updates_and_removes_cleanly() {
        let dir = tempdir().unwrap();
        let lua = dir.path().join("hyprland.lua");
        fs::write(&lua, "require(\"config/autostart\")\n").unwrap();

        set_managed_block_with_prefix(&lua, "--", "velodock blur", Some(HYPRLAND_LUA_BLUR_BODY))
            .unwrap();
        assert!(has_managed_block_with_prefix(&lua, "--", "velodock blur"));
        let raw = fs::read_to_string(&lua).unwrap();
        assert!(raw.contains("-- >>> velodock blur >>>"));
        assert!(raw.contains("hl.layer_rule({"));
        assert!(raw.contains("-- <<< velodock blur <<<"));

        set_managed_block_with_prefix(
            &lua,
            "--",
            "velodock autostart",
            Some(HYPRLAND_LUA_AUTOSTART_BODY),
        )
        .unwrap();
        assert!(has_managed_block_with_prefix(&lua, "--", "velodock autostart"));

        set_managed_block_with_prefix(&lua, "--", "velodock blur", None).unwrap();
        assert!(!has_managed_block_with_prefix(&lua, "--", "velodock blur"));
        assert!(has_managed_block_with_prefix(&lua, "--", "velodock autostart"));

        set_managed_block_with_prefix(&lua, "--", "velodock autostart", None).unwrap();
        let after = fs::read_to_string(&lua).unwrap();
        assert_eq!(after, "require(\"config/autostart\")\n");
    }
}
