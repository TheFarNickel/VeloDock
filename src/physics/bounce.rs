use crate::config::LaunchAnimation;
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy)]
pub struct LaunchPose {
    pub offset_x: f64,
    pub offset_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub glow: f64,
}

impl Default for LaunchPose {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            glow: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BounceState {
    active: bool,
    elapsed: f64,
    height: f64,
    velocity: f64,
    bounces_left: u8,
    launched: bool,
}

impl BounceState {
    pub fn trigger(&mut self, max_bounces: u8) {
        self.active = true;
        self.elapsed = 0.0;
        self.launched = false;
        self.bounces_left = max_bounces.max(1);
        self.height = 0.0;
        self.velocity = 1.0;
    }

    pub fn notify_launched(&mut self) {
        self.launched = true;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn step(&mut self, dt: f64, icon_size: f64) -> f64 {
        self.step_pose(dt, icon_size, 1.0, LaunchAnimation::Bounce)
            .offset_y
    }

    pub fn step_pose(
        &mut self,
        dt: f64,
        icon_size: f64,
        intensity: f64,
        kind: LaunchAnimation,
    ) -> LaunchPose {
        if !self.active {
            return LaunchPose::default();
        }
        let dt = dt.clamp(0.0, 0.033);
        let intensity = intensity.clamp(0.4, 1.8);
        self.elapsed += dt;

        match kind {
            LaunchAnimation::Bounce => {
                let jump_speed = icon_size * 5.2 * intensity.sqrt();
                let gravity = icon_size * 22.0;

                if (self.velocity - 1.0).abs() < 1e-6 && self.height == 0.0 {
                    self.velocity = jump_speed;
                }

                self.velocity -= gravity * dt;
                self.height += self.velocity * dt;

                let stretch = (self.velocity / jump_speed.max(1.0)).clamp(-1.0, 1.0) * 0.06;

                if self.height <= 0.0 {
                    self.height = 0.0;
                    if self.launched || self.bounces_left <= 1 {
                        self.active = false;
                        self.velocity = 0.0;
                        self.bounces_left = 0;
                        return LaunchPose::default();
                    } else {
                        self.bounces_left -= 1;
                        self.velocity = (-self.velocity * 0.72).max(jump_speed * 0.45);
                    }
                }

                LaunchPose {
                    offset_x: 0.0,
                    offset_y: self.height,
                    scale_x: 1.0 - stretch * 0.5,
                    scale_y: 1.0 + stretch * 0.5,
                    rotation: 0.0,
                    glow: 0.0,
                }
            }
            LaunchAnimation::ElasticPop => {
                let duration = 0.85;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }

                let decay = (-5.0 * t).exp();
                let osc = (t * PI * 4.5).sin() * decay * 0.28 * intensity;
                LaunchPose {
                    offset_x: 0.0,
                    offset_y: (osc.max(0.0)) * icon_size * 0.45,
                    scale_x: 1.0 + osc,
                    scale_y: 1.0 - osc * 0.65,
                    rotation: 0.0,
                    glow: (1.0 - t) * 0.35,
                }
            }
            LaunchAnimation::PulseGlow => {
                let max_dur = if self.launched { 0.8 } else { 2.0 };
                if self.elapsed >= max_dur {
                    self.active = false;
                    return LaunchPose::default();
                }
                let fade = (1.0 - (self.elapsed / max_dur)).clamp(0.0, 1.0);
                let wave = 0.5 * (1.0 - (self.elapsed * PI * 3.0).cos());
                let sc = 1.0 + wave * 0.14 * intensity * fade;
                LaunchPose {
                    offset_x: 0.0,
                    offset_y: 0.0,
                    scale_x: sc,
                    scale_y: sc,
                    rotation: 0.0,
                    glow: wave * 0.85 * fade,
                }
            }
            LaunchAnimation::Wiggle => {
                let duration = 0.78;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }
                let decay = (-3.6 * t).exp();
                let angle = (t * PI * 7.0).sin() * decay * 0.24 * intensity;
                let lift = (1.0 - t) * icon_size * 0.12 * intensity;
                LaunchPose {
                    offset_x: 0.0,
                    offset_y: lift,
                    scale_x: 1.0 + decay * 0.06,
                    scale_y: 1.0 + decay * 0.06,
                    rotation: angle,
                    glow: 0.0,
                }
            }
            LaunchAnimation::Spin360 => {
                let duration = 0.68;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }

                let eased = 1.0 - (1.0 - t).powi(3);
                let lift = (t * PI).sin() * icon_size * 0.22 * intensity;
                LaunchPose {
                    offset_x: 0.0,
                    offset_y: lift,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: eased * PI * 2.0,
                    glow: (t * PI).sin() * 0.35,
                }
            }
            LaunchAnimation::AxisFlip => {
                let duration = 0.62;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }
                let flip = (t * PI * 2.0).cos();
                let sx = flip.abs().max(0.06);
                let lift = (t * PI).sin() * icon_size * 0.26 * intensity;
                let zoom = 1.0 + (t * PI).sin() * 0.14 * intensity;
                LaunchPose {
                    offset_x: 0.0,
                    offset_y: lift,
                    scale_x: sx * zoom,
                    scale_y: zoom,
                    rotation: 0.0,
                    glow: (t * PI).sin() * 0.45,
                }
            }
            LaunchAnimation::DoublePulse => {
                let duration = 0.72;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }
                let beat1 = (-(t - 0.18).powi(2) / 0.008).exp();
                let beat2 = (-(t - 0.48).powi(2) / 0.010).exp() * 0.85;
                let pulse = (beat1 + beat2) * 0.22 * intensity;
                LaunchPose {
                    offset_x: 0.0,
                    offset_y: pulse * icon_size * 0.18,
                    scale_x: 1.0 + pulse,
                    scale_y: 1.0 + pulse,
                    rotation: 0.0,
                    glow: (beat1 + beat2).min(1.0) * 0.55,
                }
            }
            LaunchAnimation::SpringPress => {
                let duration = 0.56;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }
                let (sx, sy, oy) = if t < 0.38 {
                    let u = (t / 0.38 * PI * 0.5).sin();
                    (
                        1.0 + 0.42 * intensity * u,
                        (1.0 - 0.46 * intensity * u).max(0.35),
                        -icon_size * 0.18 * u,
                    )
                } else {
                    let u = (t - 0.38) / 0.62;
                    let rebound = (u * PI * 2.0).sin() * (-3.5 * u).exp();
                    (
                        1.0 - 0.22 * intensity * rebound,
                        1.0 + 0.26 * intensity * rebound,
                        icon_size * 0.22 * intensity * rebound.max(0.0),
                    )
                };
                LaunchPose {
                    offset_x: 0.0,
                    offset_y: oy,
                    scale_x: sx,
                    scale_y: sy,
                    rotation: 0.0,
                    glow: 0.0,
                }
            }
            LaunchAnimation::PopCelebrate => {
                let duration = 0.72;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }
                let (sc, rot) = if t < 0.2 {
                    let u = t / 0.2;
                    (1.0 - 0.12 * u, -0.08 * u)
                } else {
                    let u = (t - 0.2) / 0.8;
                    let env = (1.0 - u).powf(0.8);
                    (
                        1.0 + 0.18 * intensity * (u * PI).sin(),
                        (u * PI * 6.0).sin() * 0.14 * intensity * env,
                    )
                };
                LaunchPose {
                    offset_x: 0.0,
                    offset_y: (t * PI).sin() * icon_size * 0.14 * intensity,
                    scale_x: sc,
                    scale_y: sc,
                    rotation: rot,
                    glow: (t * PI).sin() * 0.40,
                }
            }
            LaunchAnimation::DampedSway => {
                let duration = 0.78;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }
                let decay = (-3.2 * t).exp();
                let rot = (t * PI * 4.0).sin() * 0.38 * intensity * decay;
                let ox = rot * icon_size * 0.22;
                LaunchPose {
                    offset_x: ox,
                    offset_y: rot.abs() * icon_size * 0.10,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: rot,
                    glow: 0.0,
                }
            }
            LaunchAnimation::QuickJolt => {
                let duration = 0.46;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }
                let decay = 1.0 - t;
                let ox = (t * PI * 8.0).sin() * icon_size * 0.18 * intensity * decay;
                LaunchPose {
                    offset_x: ox,
                    offset_y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    glow: 0.0,
                }
            }
            LaunchAnimation::ArcRoll => {
                let duration = 0.62;
                let t = (self.elapsed / duration).min(1.0);
                if t >= 1.0 {
                    self.active = false;
                    return LaunchPose::default();
                }
                let arc = (t * PI).sin();
                LaunchPose {
                    offset_x: arc * icon_size * 0.42 * intensity,
                    offset_y: arc * icon_size * 0.08,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: arc * PI,
                    glow: arc * 0.25,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_rises_and_settles_on_launch() {
        let mut b = BounceState::default();
        b.trigger(4);
        let mut peak = 0.0_f64;
        for _ in 0..20 {
            peak = peak.max(b.step(1.0 / 60.0, 52.0));
        }
        assert!(peak > 8.0);
        b.notify_launched();
        for _ in 0..180 {
            b.step(1.0 / 60.0, 52.0);
        }
        assert!(!b.is_active());
    }

    #[test]
    fn all_launch_animation_styles_complete_cleanly() {
        for kind in [
            LaunchAnimation::ElasticPop,
            LaunchAnimation::PulseGlow,
            LaunchAnimation::Wiggle,
            LaunchAnimation::Spin360,
            LaunchAnimation::AxisFlip,
            LaunchAnimation::DoublePulse,
            LaunchAnimation::SpringPress,
            LaunchAnimation::PopCelebrate,
            LaunchAnimation::DampedSway,
            LaunchAnimation::QuickJolt,
            LaunchAnimation::ArcRoll,
        ] {
            let mut b = BounceState::default();
            b.trigger(3);
            let mut moved = false;
            for _ in 0..180 {
                let p = b.step_pose(1.0 / 60.0, 52.0, 1.0, kind);
                if p.offset_y.abs() > 0.1
                    || p.offset_x.abs() > 0.1
                    || (p.scale_x - 1.0).abs() > 0.01
                    || p.rotation.abs() > 0.01
                    || p.glow > 0.05
                {
                    moved = true;
                }
            }
            assert!(moved, "{kind:?} should animate");
            assert!(!b.is_active(), "{kind:?} should settle");
        }
    }
}
