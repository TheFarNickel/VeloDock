use gtk4::gio;
use gtk4::glib;
use gtk4::glib::prelude::ToVariant;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SysInfoSnapshot {
    pub cpu_pct: u8,
    pub mem_pct: u8,
    pub battery_pct: Option<u8>,
    pub battery_charging: bool,
}

#[derive(Default)]
pub struct SysInfoReader {
    prev_total: u64,
    prev_idle: u64,
}

impl SysInfoReader {
    pub fn sample(&mut self) -> SysInfoSnapshot {
        let cpu_pct = self.sample_cpu();
        let mem_pct = sample_mem_pct();
        let (battery_pct, battery_charging) = sample_battery();
        SysInfoSnapshot {
            cpu_pct,
            mem_pct,
            battery_pct,
            battery_charging,
        }
    }

    fn sample_cpu(&mut self) -> u8 {
        let Ok(raw) = fs::read_to_string("/proc/stat") else {
            return 0;
        };
        let Some(first) = raw.lines().next() else {
            return 0;
        };
        let nums: Vec<u64> = first
            .split_whitespace()
            .skip(1)
            .filter_map(|s| s.parse::<u64>().ok())
            .collect();
        if nums.len() < 4 {
            return 0;
        }
        let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
        let total: u64 = nums.iter().sum();
        let pct = if self.prev_total > 0 && total > self.prev_total {
            let dt = total - self.prev_total;
            let di = idle.saturating_sub(self.prev_idle);
            (((dt.saturating_sub(di)) as f64 / dt as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8
        } else {
            0
        };
        self.prev_total = total;
        self.prev_idle = idle;
        pct
    }
}

fn sample_mem_pct() -> u8 {
    let Ok(raw) = fs::read_to_string("/proc/meminfo") else {
        return 0;
    };
    let mut total_kb = 0_u64;
    let mut avail_kb = 0_u64;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
    }
    if total_kb == 0 {
        return 0;
    }
    let used = total_kb.saturating_sub(avail_kb);
    ((used as f64 / total_kb as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

fn sample_battery() -> (Option<u8>, bool) {
    let base = Path::new("/sys/class/power_supply");
    let Ok(entries) = fs::read_dir(base) else {
        return (None, false);
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("BAT") {
            continue;
        }
        let dir = entry.path();
        let cap = fs::read_to_string(dir.join("capacity"))
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .map(|v| v.min(100));
        if let Some(pct) = cap {
            let status = fs::read_to_string(dir.join("status"))
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            let charging = status == "charging" || status == "full";
            return (Some(pct), charging);
        }
    }
    (None, false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSnapshot {
    pub bus_name: String,
    pub title: String,
    pub artist: String,
    pub art_path: Option<String>,
    pub playing: bool,
}

pub fn query_mpris() -> Option<MediaSnapshot> {
    let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let res = conn
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
            None,
            None,
            gio::DBusCallFlags::NONE,
            180,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let names_var = res.child_value(0);
    let players: Vec<String> = (0..names_var.n_children())
        .filter_map(|i| names_var.child_value(i).str().map(str::to_string))
        .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
        .collect();

    let mut fallback: Option<MediaSnapshot> = None;
    for bus in players {
        let status = dbus_get_prop(&conn, &bus, "org.mpris.MediaPlayer2.Player", "PlaybackStatus")
            .and_then(|v| v.str().map(str::to_string))
            .unwrap_or_default();
        if status.eq_ignore_ascii_case("stopped") || status.is_empty() {
            continue;
        }
        let playing = status.eq_ignore_ascii_case("playing");
        let (title, artist, art_path) =
            dbus_get_prop(&conn, &bus, "org.mpris.MediaPlayer2.Player", "Metadata")
                .map(extract_mpris_metadata)
                .unwrap_or_default();
        let display_title = if !title.is_empty() {
            title
        } else {
            bus.trim_start_matches("org.mpris.MediaPlayer2.")
                .split('.')
                .next()
                .unwrap_or("Media")
                .to_string()
        };
        let snap = MediaSnapshot {
            bus_name: bus,
            title: display_title,
            artist,
            art_path,
            playing,
        };
        if playing {
            return Some(snap);
        }
        if fallback.is_none() {
            fallback = Some(snap);
        }
    }
    fallback
}

fn dbus_get_prop(
    conn: &gio::DBusConnection,
    bus_name: &str,
    iface: &str,
    prop: &str,
) -> Option<glib::Variant> {
    let params = glib::Variant::tuple_from_iter([
        glib::Variant::from(iface),
        glib::Variant::from(prop),
    ]);
    let reply = conn
        .call_sync(
            Some(bus_name),
            "/org/mpris/MediaPlayer2",
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            180,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let outer = reply.child_value(0);
    outer.as_variant()
}

fn extract_mpris_metadata(meta: glib::Variant) -> (String, String, Option<String>) {
    let mut title = String::new();
    let mut artist = String::new();
    let mut art_path = None;
    for i in 0..meta.n_children() {
        let entry = meta.child_value(i);
        if entry.n_children() < 2 {
            continue;
        }
        let Some(k) = entry.child_value(0).str().map(str::to_string) else {
            continue;
        };
        let Some(val) = entry.child_value(1).as_variant() else {
            continue;
        };
        if k == "xesam:title" {
            if let Some(s) = val.str() {
                title = s.to_string();
            }
        } else if k == "xesam:artist" {
            if val.n_children() > 0 {
                if let Some(s) = val.child_value(0).str() {
                    artist = s.to_string();
                }
            } else if let Some(s) = val.str() {
                artist = s.to_string();
            }
        } else if k == "mpris:artUrl" {
            if let Some(url) = val.str() {
                if let Some(local) = url.strip_prefix("file://") {
                    let decoded = percent_encoding::percent_decode_str(local)
                        .decode_utf8_lossy()
                        .into_owned();
                    if Path::new(&decoded).exists() {
                        art_path = Some(decoded);
                    }
                }
            }
        }
    }
    (title, artist, art_path)
}

pub fn mpris_command(bus_name: &str, method: &str) {
    let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return;
    };
    let _ = conn.call_sync(
        Some(bus_name),
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
        method,
        None,
        None,
        gio::DBusCallFlags::NONE,
        180,
        gio::Cancellable::NONE,
    );
}

pub fn mpris_adjust_volume(bus_name: &str, delta: f64) {
    let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return;
    };
    let cur = dbus_get_prop(&conn, bus_name, "org.mpris.MediaPlayer2.Player", "Volume")
        .and_then(|v| v.get::<f64>())
        .unwrap_or(0.5);
    let next = (cur + delta).clamp(0.0, 1.0);
    let boxed = glib::Variant::from(next).to_variant();
    let params = glib::Variant::tuple_from_iter([
        glib::Variant::from("org.mpris.MediaPlayer2.Player"),
        glib::Variant::from("Volume"),
        boxed,
    ]);
    let _ = conn.call_sync(
        Some(bus_name),
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "Set",
        Some(&params),
        None,
        gio::DBusCallFlags::NONE,
        180,
        gio::Cancellable::NONE,
    );
}

pub fn eval_math_expr(input: &str) -> Option<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || !trimmed.chars().any(|c| matches!(c, '+' | '-' | '*' | '/' | '^' | '('))
    {
        return None;
    }
    let bytes: Vec<u8> = trimmed
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if bytes.is_empty()
        || !bytes
            .iter()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'*' | b'/' | b'^' | b'(' | b')' | b'.'))
    {
        return None;
    }
    let mut pos = 0;
    let val = parse_add_sub(&bytes, &mut pos)?;
    if pos == bytes.len() && val.is_finite() {
        Some(val)
    } else {
        None
    }
}

fn parse_add_sub(b: &[u8], pos: &mut usize) -> Option<f64> {
    let mut lhs = parse_mul_div(b, pos)?;
    while *pos < b.len() {
        match b[*pos] {
            b'+' => {
                *pos += 1;
                lhs += parse_mul_div(b, pos)?;
            }
            b'-' => {
                *pos += 1;
                lhs -= parse_mul_div(b, pos)?;
            }
            _ => break,
        }
    }
    Some(lhs)
}

fn parse_mul_div(b: &[u8], pos: &mut usize) -> Option<f64> {
    let mut lhs = parse_pow(b, pos)?;
    while *pos < b.len() {
        match b[*pos] {
            b'*' => {
                *pos += 1;
                lhs *= parse_pow(b, pos)?;
            }
            b'/' => {
                *pos += 1;
                let rhs = parse_pow(b, pos)?;
                if rhs.abs() < 1e-12 {
                    return None;
                }
                lhs /= rhs;
            }
            _ => break,
        }
    }
    Some(lhs)
}

fn parse_pow(b: &[u8], pos: &mut usize) -> Option<f64> {
    let base = parse_unary(b, pos)?;
    if *pos < b.len() && b[*pos] == b'^' {
        *pos += 1;
        let exp = parse_pow(b, pos)?;
        Some(base.powf(exp))
    } else {
        Some(base)
    }
}

fn parse_unary(b: &[u8], pos: &mut usize) -> Option<f64> {
    if *pos < b.len() && b[*pos] == b'-' {
        *pos += 1;
        return Some(-parse_unary(b, pos)?);
    }
    if *pos < b.len() && b[*pos] == b'+' {
        *pos += 1;
        return parse_unary(b, pos);
    }
    parse_primary(b, pos)
}

fn parse_primary(b: &[u8], pos: &mut usize) -> Option<f64> {
    if *pos >= b.len() {
        return None;
    }
    if b[*pos] == b'(' {
        *pos += 1;
        let val = parse_add_sub(b, pos)?;
        if *pos < b.len() && b[*pos] == b')' {
            *pos += 1;
            return Some(val);
        }
        return None;
    }
    let start = *pos;
    while *pos < b.len() && (b[*pos].is_ascii_digit() || b[*pos] == b'.') {
        *pos += 1;
    }
    if *pos == start {
        return None;
    }
    std::str::from_utf8(&b[start..*pos]).ok()?.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysinfo_reader_samples_valid_percentages() {
        let mut reader = SysInfoReader::default();
        let snap = reader.sample();
        assert!(snap.cpu_pct <= 100);
        assert!(snap.mem_pct <= 100);
        if let Some(bat) = snap.battery_pct {
            assert!(bat <= 100);
        }
    }

    #[test]
    fn math_expr_evaluates_correctly() {
        assert!((eval_math_expr("25*4").unwrap() - 100.0).abs() < 1e-6);
        assert!((eval_math_expr("(1920 - 16) / 2").unwrap() - 952.0).abs() < 1e-6);
        assert!((eval_math_expr("2^10").unwrap() - 1024.0).abs() < 1e-6);
        assert!(eval_math_expr("firefox").is_none());
        assert!(eval_math_expr("10/0").is_none());
    }
}
