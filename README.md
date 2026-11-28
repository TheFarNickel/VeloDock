# VeloDock

A native **Rust + GTK4** desktop Dock for Arch Linux supporting both **Wayland** (`gtk4-layer-shell`: Hyprland, Sway, Niri, Wayfire) and **X11** (XFCE, KDE-X11, i3, Openbox).

## Features

- **7 Live Dock Styles:** `Floating Glass`, `Attached Shelf`, `3D Glass Deck`, `Full Panel`, `Dynamic Cyber Neon` (animated rotating RGB), `Dynamic Capsule` (crystal-transparent pill), and `Floating Minimal`.
- **Ultra-Smooth Spring Physics (60/120/144Hz):** Critically-damped cursor wave smoothing, cosine magnification, 5 launch animations, drag-to-reorder spring slide, feel presets (`Snappy` / `Balanced` / `Ultra Silky`), and 0% idle CPU.
- **Window Tracking:** Live running-app indicators, multi-window tooltips (`Firefox (3 windows)`), click/scroll to focus & cycle windows (Hyprland IPC, Sway IPC, X11 EWMH).
- **Launchpad, Folder Stacks, XDG Trash & Built-in Clock:** Animated fullscreen searchable Launchpad with pagination & categories (`ESC` to close), Folder Stacks, live Trash icon with drag-to-trash, and embedded Clock & Date widget.
- **Modern Settings & Autostart:** Two-pane sidebar Settings with live visual style previews, 1-click Hyprland blur & autostart, and Export/Import config.

## Install on Arch Linux

### Option 1: Build & install with `makepkg`
```bash
cd packaging
makepkg -si
```

### Option 2: Run directly with Cargo
```bash
cargo build --release
./target/release/velodock
```

## CLI Usage

```bash
velodock                     # Start the Dock + open Settings window
velodock --dock-only         # Start ONLY the Dock silently (used for Autostart)
velodock --settings          # Open/focus the Settings window
velodock --toggle-launchpad  # Toggle fullscreen Launchpad overlay
```

## Hyprland Frosted Glass Blur & Autostart

Add to `~/.config/hypr/hyprland.conf`:

```ini
exec-once = velodock --dock-only
layerrule = blur, velodock
layerrule = ignorezero, velodock
layerrule = blur, velodock-launchpad
```

## Configuration

Stored at `~/.config/velodock/config.toml` (auto-created on first run with detected installed apps).
