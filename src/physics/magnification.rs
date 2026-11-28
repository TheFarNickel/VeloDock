use crate::config::HoverAnimation;
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotGeometry {
    pub center: f64,
    pub size: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoverTarget {
    pub scale: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub skew: f64,
    pub lift: f64,
    pub shift: f64,
    pub angle: f64,
    pub glow: f64,
}

impl HoverTarget {
    pub const REST: Self = Self {
        scale: 1.0,
        scale_x: 1.0,
        scale_y: 1.0,
        skew: 0.0,
        lift: 0.0,
        shift: 0.0,
        angle: 0.0,
        glow: 0.0,
    };
}

impl Default for HoverTarget {
    fn default() -> Self {
        Self::REST
    }
}

pub fn compute_scales(
    centers: &[f64],
    pointer: Option<f64>,
    icon_size: f64,
    max_scale: f64,
    hover_factor: f64,
) -> Vec<f64> {
    compute_scales_with_spread(centers, pointer, icon_size, max_scale, hover_factor, 2.6)
}

pub fn compute_scales_with_spread(
    centers: &[f64],
    pointer: Option<f64>,
    icon_size: f64,
    max_scale: f64,
    hover_factor: f64,
    spread: f64,
) -> Vec<f64> {
    compute_hover_targets(
        centers,
        pointer,
        icon_size,
        8.0,
        max_scale,
        hover_factor,
        spread,
        HoverAnimation::Wave,
        0.0,
        0.0,
    )
    .into_iter()
    .map(|t| t.scale)
    .collect()
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[allow(clippy::too_many_arguments)]
pub fn compute_hover_targets(
    centers: &[f64],
    pointer: Option<f64>,
    icon_size: f64,
    gap: f64,
    max_scale: f64,
    hover_factor: f64,
    spread: f64,
    style: HoverAnimation,
    pointer_velocity: f64,
    time_sec: f64,
) -> Vec<HoverTarget> {
    let clamped_hover = hover_factor.clamp(0.0, 1.0);
    let max_scale = max_scale.max(1.0);
    let delta = max_scale - 1.0;

    let Some(ptr) = pointer else {
        return vec![HoverTarget::REST; centers.len()];
    };
    if clamped_hover <= 0.0 {
        return vec![HoverTarget::REST; centers.len()];
    }
    if delta <= 0.0001 && matches!(style, HoverAnimation::Wave | HoverAnimation::Spotlight) {
        return vec![HoverTarget::REST; centers.len()];
    }

    let slot_step = (icon_size + gap).max(1.0);
    let radius = (icon_size * spread.clamp(1.2, 5.0)).max(1.0);

    centers
        .iter()
        .map(|&c| {
            let signed_dx = c - ptr;
            let dist = signed_dx.abs();
            let u = dist / slot_step;
            let ratio = (dist / radius).min(1.0);
            let sign = if signed_dx > 0.5 {
                1.0
            } else if signed_dx < -0.5 {
                -1.0
            } else {
                0.0
            };

            match style {
                HoverAnimation::Wave => {
                    let factor = if dist < radius {
                        0.5 * (1.0 + (PI * ratio).cos())
                    } else {
                        0.0
                    };
                    HoverTarget {
                        scale: 1.0 + delta * factor * clamped_hover,
                        ..HoverTarget::REST
                    }
                }
                HoverAnimation::Spotlight => {
                    let core = if u < 0.85 {
                        (0.5 * (1.0 + (PI * u / 0.85).cos())).powi(2)
                    } else {
                        0.0
                    };
                    let halo = if u < 1.75 {
                        0.5 * (1.0 + (PI * u / 1.75).cos())
                    } else {
                        0.0
                    };
                    HoverTarget {
                        scale: 1.0 + delta * (core * 0.94 + halo * 0.24) * clamped_hover,
                        lift: icon_size * (core * 0.14 + halo * 0.22) * clamped_hover,
                        glow: core * 0.28 * clamped_hover,
                        ..HoverTarget::REST
                    }
                }
                HoverAnimation::CascadeLean => {
                    if dist >= radius {
                        return HoverTarget::REST;
                    }
                    let bell = (0.5 * (1.0 + (PI * ratio).cos())).powf(1.25);
                    let tilt_env = (PI * ratio).sin();
                    HoverTarget {
                        scale: 1.0 + delta * bell * clamped_hover,
                        lift: icon_size * 0.20 * bell * clamped_hover,
                        angle: -sign * 0.32 * tilt_env * clamped_hover,
                        ..HoverTarget::REST
                    }
                }
                HoverAnimation::OrbitalArc => {
                    if dist >= radius {
                        return HoverTarget::REST;
                    }
                    let drum_theta = (signed_dx / radius).clamp(-1.0, 1.0) * (PI * 0.42);
                    let arc_depth = ((1.0 - ratio) * (PI * 0.5)).sin().powf(1.35);
                    let sx = (drum_theta.cos()).clamp(0.58, 1.0);
                    let skew = -drum_theta.sin() * 0.18 * clamped_hover;
                    HoverTarget {
                        scale: 1.0 + delta * arc_depth * clamped_hover,
                        scale_x: 1.0 - (1.0 - sx) * clamped_hover,
                        scale_y: 1.0,
                        skew,
                        lift: icon_size * 0.24 * arc_depth * clamped_hover,
                        ..HoverTarget::REST
                    }
                }
                HoverAnimation::PrismDeck => {
                    if dist >= radius {
                        return HoverTarget::REST;
                    }
                    let plateau = 1.0 - smoothstep(0.0, 1.0, ratio.powf(1.3));
                    let yaw = smoothstep(0.05, 0.85, ratio) * (1.0 - ratio * 0.35);
                    let sx = (1.0 - 0.36 * yaw * clamped_hover).clamp(0.55, 1.0);
                    let skew = sign * 0.26 * yaw * clamped_hover;
                    let shift = sign * icon_size * 0.14 * yaw * clamped_hover;
                    HoverTarget {
                        scale: 1.0 + delta * plateau * clamped_hover,
                        scale_x: sx,
                        scale_y: 1.0,
                        skew,
                        lift: icon_size * 0.28 * plateau * clamped_hover,
                        shift,
                        glow: plateau * 0.25 * clamped_hover,
                        ..HoverTarget::REST
                    }
                }
                HoverAnimation::RubberFlex => {
                    if dist >= radius {
                        return HoverTarget::REST;
                    }
                    let bell = 0.5 * (1.0 + (PI * ratio).cos());
                    let vel_drive = (pointer_velocity / 900.0).clamp(-1.0, 1.0);
                    let pos_drive = (-signed_dx / radius * PI).sin();
                    let pull = (pos_drive * 0.55 + vel_drive * 0.45) * bell * clamped_hover;
                    let stretch = pull.abs() * 0.26;
                    let squash = pull.abs() * 0.18;
                    let (sx, sy) = if pull >= 0.0 {
                        (1.0 + stretch, (1.0 - squash).max(0.65))
                    } else {
                        ((1.0 - squash).max(0.65), 1.0 + stretch)
                    };
                    HoverTarget {
                        scale: 1.0 + delta * bell * clamped_hover,
                        scale_x: sx,
                        scale_y: sy,
                        lift: icon_size * 0.22 * bell * clamped_hover,
                        shift: pull * icon_size * 0.32,
                        ..HoverTarget::REST
                    }
                }
                HoverAnimation::MagnetSplit => {
                    let core = if u < 0.65 {
                        0.5 * (1.0 + (PI * u / 0.65).cos())
                    } else {
                        0.0
                    };
                    let repel = if u > 0.45 && u < 3.4 {
                        let t = ((u - 0.45) / 2.95).clamp(0.0, 1.0);
                        (PI * t).sin() * (1.0 - t * 0.45)
                    } else {
                        0.0
                    };
                    let attract_shift = -signed_dx * 0.22 * core * clamped_hover;
                    let repel_shift = sign * icon_size * 0.34 * repel * clamped_hover;
                    let shrink = -0.11 * repel * clamped_hover;
                    HoverTarget {
                        scale: (1.0 + delta * 0.72 * core * clamped_hover + shrink).max(0.84),
                        lift: icon_size * (0.28 * core - 0.04 * repel) * clamped_hover,
                        shift: attract_shift + repel_shift,
                        angle: (-signed_dx / slot_step).clamp(-1.0, 1.0) * 0.10 * core * clamped_hover,
                        glow: core * 0.55 * clamped_hover,
                        ..HoverTarget::REST
                    }
                }
                HoverAnimation::InertiaSway => {
                    let near = if u < 2.8 {
                        0.5 * (1.0 + (PI * u / 2.8).cos())
                    } else {
                        0.0
                    };
                    let vel_norm = (pointer_velocity / 900.0).clamp(-1.0, 1.0);
                    let pos_tilt = (-signed_dx / (slot_step * 2.0)).clamp(-1.0, 1.0) * 0.14;
                    let vel_swing = vel_norm * 0.44 * near;
                    HoverTarget {
                        scale: 1.0 + delta * 0.42 * near * clamped_hover,
                        lift: icon_size * (0.14 * near + vel_norm.abs() * 0.16 * near) * clamped_hover,
                        shift: vel_norm * icon_size * 0.14 * near * clamped_hover,
                        angle: (vel_swing + pos_tilt * near) * clamped_hover,
                        glow: (vel_norm.abs() * 0.45 * near) * clamped_hover,
                        ..HoverTarget::REST
                    }
                }
                HoverAnimation::PulseWave => {
                    let wave_env = (-0.28 * u * u).exp();
                    let phase = u * 2.5 - time_sec * 6.5;
                    let ripple = phase.cos() * wave_env;
                    let crest = (ripple + 0.35 * wave_env).max(0.0);
                    let core = (-1.6 * u * u).exp();
                    HoverTarget {
                        scale: (1.0 + (delta * 0.38 * core + 0.12 * ripple) * clamped_hover).max(0.88),
                        lift: icon_size * (0.28 * crest + 0.10 * core) * clamped_hover,
                        shift: phase.sin() * icon_size * 0.06 * wave_env * clamped_hover,
                        angle: phase.sin() * 0.09 * wave_env * clamped_hover,
                        glow: (0.50 * core + 0.32 * crest).clamp(0.0, 0.85) * clamped_hover,
                        ..HoverTarget::REST
                    }
                }
            }
        })
        .collect()
}

pub fn layout_slots(
    scales: &[f64],
    icon_size: f64,
    gap: f64,
    separator_after: &[usize],
    sep_width: f64,
) -> (Vec<SlotGeometry>, f64) {
    let mut slots = Vec::with_capacity(scales.len());
    let mut cursor = 0.0_f64;

    for (idx, &scale) in scales.iter().enumerate() {
        let s = scale.max(0.0);
        let size = icon_size * s;
        let center = cursor + size * 0.5;
        slots.push(SlotGeometry {
            center,
            size,
            scale: s,
        });
        cursor += size;
        if idx + 1 < scales.len() {
            if separator_after.contains(&idx) {
                cursor += gap + sep_width + gap;
            } else {
                cursor += gap;
            }
        }
    }

    (slots, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnification_is_symmetric_and_bounded() {
        let centers = vec![0.0, 60.0, 120.0, 180.0, 240.0];
        let scales = compute_scales(&centers, Some(120.0), 52.0, 1.65, 1.0);
        assert!((scales[2] - 1.65).abs() < 1e-6);
        assert!((scales[1] - scales[3]).abs() < 1e-6);
        assert!(scales[0] >= 1.0 && scales[0] < scales[1]);
    }

    #[test]
    fn layout_slots_is_strictly_monotonic_with_separators() {
        let scales = vec![1.0, 1.4, 1.0, 1.0];
        let (slots, total) = layout_slots(&scales, 50.0, 8.0, &[1], 10.0);
        assert_eq!(slots.len(), 4);
        for pair in slots.windows(2) {
            assert!(pair[1].center > pair[0].center);
        }
        assert!(total > 200.0);
    }

    #[test]
    fn spotlight_peaks_center_and_lifts_immediate_neighbors_only() {
        let centers = vec![0.0, 60.0, 120.0, 180.0, 240.0];
        let targets = compute_hover_targets(
            &centers,
            Some(120.0),
            52.0,
            8.0,
            1.65,
            1.0,
            2.6,
            HoverAnimation::Spotlight,
            0.0,
            0.0,
        );
        assert!(targets[2].scale > 1.70);
        assert!(targets[1].lift > 1.0 && targets[3].lift > 1.0);
        assert!((targets[1].scale - targets[3].scale).abs() < 1e-6);
        assert!((targets[0].scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn prism_and_orbital_foreshorten_flanks() {
        let centers = vec![0.0, 60.0, 120.0, 180.0, 240.0];
        for style in [HoverAnimation::PrismDeck, HoverAnimation::OrbitalArc] {
            let targets = compute_hover_targets(
                &centers,
                Some(120.0),
                52.0,
                8.0,
                1.65,
                1.0,
                2.6,
                style,
                0.0,
                0.0,
            );
            assert!((targets[2].scale_x - 1.0).abs() < 1e-4);
            assert!(targets[1].scale_x < 0.96);
            assert!(targets[3].scale_x < 0.96);
            assert!((targets[1].skew + targets[3].skew).abs() < 1e-6);
        }
    }

    #[test]
    fn magnet_split_repels_neighbors_outward() {
        let centers = vec![0.0, 60.0, 120.0, 180.0, 240.0];
        let targets = compute_hover_targets(
            &centers,
            Some(120.0),
            52.0,
            8.0,
            1.65,
            1.0,
            2.6,
            HoverAnimation::MagnetSplit,
            0.0,
            0.0,
        );
        assert!(targets[2].scale > 1.3);
        assert!(targets[1].shift < -5.0);
        assert!(targets[3].shift > 5.0);
    }

    #[test]
    fn inertia_sway_reacts_to_pointer_velocity() {
        let centers = vec![0.0, 60.0, 120.0, 180.0, 240.0];
        let fast = compute_hover_targets(
            &centers,
            Some(120.0),
            52.0,
            8.0,
            1.65,
            1.0,
            2.6,
            HoverAnimation::InertiaSway,
            800.0,
            0.0,
        );
        let still = compute_hover_targets(
            &centers,
            Some(120.0),
            52.0,
            8.0,
            1.65,
            1.0,
            2.6,
            HoverAnimation::InertiaSway,
            0.0,
            0.0,
        );
        assert!(fast[2].angle.abs() > still[2].angle.abs() + 0.15);
    }
}
