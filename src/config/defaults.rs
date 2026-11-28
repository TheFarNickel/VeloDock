use std::collections::HashSet;

const PRIORITY_GROUPS: &[&[&str]] = &[

    &[
        "firefox.desktop",
        "chromium.desktop",
        "google-chrome.desktop",
        "brave-browser.desktop",
        "zen.desktop",
        "vivaldi-stable.desktop",
    ],

    &[
        "kitty.desktop",
        "alacritty.desktop",
        "com.mitchellh.ghostty.desktop",
        "org.wezfurlong.wezterm.desktop",
        "foot.desktop",
        "org.kde.konsole.desktop",
        "org.gnome.Terminal.desktop",
    ],

    &[
        "org.gnome.Nautilus.desktop",
        "org.kde.dolphin.desktop",
        "thunar.desktop",
        "nemo.desktop",
        "pcmanfm.desktop",
    ],

    &[
        "code.desktop",
        "code-oss.desktop",
        "codium.desktop",
        "dev.zed.Zed.desktop",
        "neovide.desktop",
    ],

    &[
        "spotify.desktop",
        "discord.desktop",
        "org.telegram.desktop.desktop",
        "vlc.desktop",
    ],
];

pub fn detect_default_pinned(available_ids: &[String]) -> Vec<String> {
    let available: HashSet<&str> = available_ids.iter().map(String::as_str).collect();
    let mut picked = Vec::new();
    for group in PRIORITY_GROUPS {
        if let Some(&id) = group.iter().find(|&&candidate| available.contains(candidate)) {
            picked.push(id.to_string());
        }
    }
    picked
}
