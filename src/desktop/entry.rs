use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub startup_wm_class: Option<String>,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
    pub categories: Vec<String>,
    pub path: PathBuf,
}

const FIELD_CODES: &[&str] = &[
    "%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%i", "%c", "%k", "%v", "%m",
];

impl DesktopEntry {
    pub fn parse_str(id: &str, path: PathBuf, content: &str) -> Option<Self> {
        let mut in_desktop_entry = false;
        let mut entry_type: Option<String> = None;
        let mut name: Option<String> = None;
        let mut exec: Option<String> = None;
        let mut icon: Option<String> = None;
        let mut startup_wm_class: Option<String> = None;
        let mut no_display = false;
        let mut hidden = false;
        let mut terminal = false;
        let mut categories = Vec::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                if in_desktop_entry {
                    break;
                }
                in_desktop_entry = line == "[Desktop Entry]";
                continue;
            }
            if !in_desktop_entry {
                continue;
            }
            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let val = val.trim();
            match key {
                "Type" => entry_type = Some(val.to_string()),
                "Name" => {
                    if name.is_none() {
                        name = Some(val.to_string());
                    }
                }
                "Exec" => exec = Some(val.to_string()),
                "Icon" => icon = Some(val.to_string()),
                "StartupWMClass" => startup_wm_class = Some(val.to_string()),
                "NoDisplay" => no_display = val.eq_ignore_ascii_case("true"),
                "Hidden" => hidden = val.eq_ignore_ascii_case("true"),
                "Terminal" => terminal = val.eq_ignore_ascii_case("true"),
                "Categories" => {
                    categories = val
                        .split(';')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                _ => {}
            }
        }

        if entry_type.as_deref() != Some("Application") {
            return None;
        }
        let name = name?;
        let exec = exec?;
        if exec.is_empty() {
            return None;
        }

        Some(Self {
            id: id.to_string(),
            name,
            exec,
            icon,
            startup_wm_class,
            no_display,
            hidden,
            terminal,
            categories,
            path,
        })
    }

    pub fn exec_args(&self) -> Vec<String> {
        let parts = shlex::split(&self.exec).unwrap_or_else(|| {
            self.exec
                .split_whitespace()
                .map(ToString::to_string)
                .collect()
        });
        parts
            .into_iter()
            .filter(|arg| !FIELD_CODES.contains(&arg.as_str()))
            .collect()
    }

    pub fn exec_binary_name(&self) -> Option<String> {
        let args = self.exec_args();
        let first = args.first()?;
        std::path::Path::new(first)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
    }

    pub fn launch(&self) -> io::Result<()> {
        let args = self.exec_args();
        if args.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty Exec command",
            ));
        }
        let mut cmd = if self.terminal {
            let term = std::env::var("TERMINAL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    for candidate in ["kitty", "alacritty", "foot", "konsole", "xterm"] {
                        if Command::new("which")
                            .arg(candidate)
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false)
                        {
                            return candidate.to_string();
                        }
                    }
                    "xterm".to_string()
                });
            let mut c = Command::new(term);
            c.arg("-e").args(&args);
            c
        } else {
            let mut c = Command::new(&args[0]);
            c.args(&args[1..]);
            c
        };

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.spawn()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::DesktopIndex;

    #[test]
    fn parses_quoted_exec_and_strips_field_codes() {
        let content = "[Desktop Entry]\nType=Application\nName=My App\nExec=\"/opt/My App/run\" --flag %U --extra %f\nIcon=myapp\nStartupWMClass=MyAppClass\n";
        let entry =
            DesktopEntry::parse_str("myapp.desktop", PathBuf::from("/tmp/myapp.desktop"), content)
                .unwrap();
        assert_eq!(entry.name, "My App");
        assert_eq!(
            entry.exec_args(),
            vec!["/opt/My App/run", "--flag", "--extra"]
        );
        assert_eq!(entry.startup_wm_class.as_deref(), Some("MyAppClass"));
    }

    #[test]
    fn filters_nodisplay_and_matches_wm_class_case_insensitive() {
        let e1 = DesktopEntry::parse_str(
            "code.desktop",
            PathBuf::new(),
            "[Desktop Entry]\nType=Application\nName=VS Code\nExec=/usr/bin/code %F\nStartupWMClass=Code\n",
        )
        .unwrap();
        let e2 = DesktopEntry::parse_str(
            "hidden.desktop",
            PathBuf::new(),
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=true\nNoDisplay=true\n",
        )
        .unwrap();
        let idx = DesktopIndex::from_entries(vec![e1, e2]);
        assert_eq!(idx.visible_sorted().len(), 1);
        assert_eq!(idx.match_app_id("code").unwrap().id, "code.desktop");
        assert_eq!(idx.match_app_id("CODE").unwrap().id, "code.desktop");
    }

    #[test]
    fn ignores_malformed_or_non_application_entries() {
        assert!(DesktopEntry::parse_str("link.desktop", PathBuf::new(), "[Desktop Entry]\nType=Link\nName=Web\n").is_none());
        assert!(DesktopEntry::parse_str("empty.desktop", PathBuf::new(), "[Desktop Entry]\nType=Application\nName=Broken\n").is_none());
    }
}
