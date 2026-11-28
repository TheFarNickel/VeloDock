pub mod bounce;
pub mod magnification;
pub mod spring;

pub use bounce::{BounceState, LaunchPose};
pub use magnification::{
    compute_hover_targets, compute_scales, compute_scales_with_spread, layout_slots, HoverTarget,
    SlotGeometry,
};
pub use spring::Spring;

use crate::config::LaunchAnimation;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ItemAnimState {
    pub scale: Spring,
    pub hover_sx: Spring,
    pub hover_sy: Spring,
    pub hover_skew: Spring,
    pub hover_lift: Spring,
    pub hover_shift: Spring,
    pub hover_angle: Spring,
    pub hover_glow: Spring,
    pub presence: Spring,
    pub drag_shift: Spring,
    pub bounce: BounceState,
    pub launch_pose: LaunchPose,
}

impl ItemAnimState {
    pub fn new(initial_presence: f64) -> Self {
        let mut presence = Spring::new(initial_presence, 260.0, 26.0);
        presence.target = 1.0;
        Self {
            scale: Spring::new(1.0, 320.0, 28.0),
            hover_sx: Spring::new(1.0, 300.0, 26.0),
            hover_sy: Spring::new(1.0, 300.0, 26.0),
            hover_skew: Spring::new(0.0, 280.0, 24.0),
            hover_lift: Spring::new(0.0, 320.0, 28.0),
            hover_shift: Spring::new(0.0, 280.0, 25.0),
            hover_angle: Spring::new(0.0, 240.0, 16.0),
            hover_glow: Spring::new(0.0, 240.0, 26.0),
            presence,
            drag_shift: Spring::new(0.0, 300.0, 28.0),
            bounce: BounceState::default(),
            launch_pose: LaunchPose::default(),
        }
    }

    pub fn is_settled(&self) -> bool {
        self.scale.is_settled()
            && self.hover_sx.is_settled()
            && self.hover_sy.is_settled()
            && self.hover_skew.is_settled()
            && self.hover_lift.is_settled()
            && self.hover_shift.is_settled()
            && self.hover_angle.is_settled()
            && self.hover_glow.is_settled()
            && self.presence.is_settled()
            && self.drag_shift.is_settled()
            && !self.bounce.is_active()
    }
}

#[derive(Debug, Clone)]
pub struct DockPhysics {
    pub hover_spring: Spring,
    pub reveal_spring: Spring,
    pub pointer_spring: Spring,
    pub pointer_initialized: bool,
    pub smoothness: f64,
    pub bounce_intensity: f64,
    pub launch_animation: LaunchAnimation,
    pub items: HashMap<String, ItemAnimState>,
}

impl Default for DockPhysics {
    fn default() -> Self {
        Self::new()
    }
}

impl DockPhysics {
    pub fn new() -> Self {
        Self {
            hover_spring: Spring::new(0.0, 220.0, 27.0),
            reveal_spring: Spring::new(1.0, 220.0, 28.0),
            pointer_spring: Spring::new(0.0, 480.0, 42.0),
            pointer_initialized: false,
            smoothness: 1.0,
            bounce_intensity: 1.0,
            launch_animation: LaunchAnimation::Bounce,
            items: HashMap::new(),
        }
    }

    pub fn set_tuning(
        &mut self,
        smoothness: f64,
        bounce_intensity: f64,
        launch_animation: LaunchAnimation,
    ) {
        let s = smoothness.clamp(0.3, 2.0);
        self.smoothness = s;
        self.bounce_intensity = bounce_intensity.clamp(0.4, 1.8);
        self.launch_animation = launch_animation;

        let tune = |spring: &mut Spring, base_k: f64, damp_ratio: f64| {
            let k = base_k / (s * s).sqrt();
            spring.stiffness = k;
            spring.damping = 2.0 * k.sqrt() * damp_ratio;
        };

        tune(&mut self.hover_spring, 220.0, 0.94);
        tune(&mut self.reveal_spring, 210.0, 0.96);
        tune(&mut self.pointer_spring, 520.0, 0.98);
        for item in self.items.values_mut() {
            tune(&mut item.scale, 320.0, 0.94);
            tune(&mut item.hover_sx, 300.0, 0.88);
            tune(&mut item.hover_sy, 300.0, 0.88);
            tune(&mut item.hover_skew, 280.0, 0.90);
            tune(&mut item.hover_lift, 320.0, 0.90);
            tune(&mut item.hover_shift, 280.0, 0.88);
            tune(&mut item.hover_angle, 240.0, 0.56);
            tune(&mut item.hover_glow, 240.0, 0.96);
            tune(&mut item.presence, 250.0, 0.92);
            tune(&mut item.drag_shift, 290.0, 0.94);
        }
    }

    pub fn update_pointer_target(&mut self, axis_pos: Option<f64>) {
        if let Some(p) = axis_pos {
            if !self.pointer_initialized {
                self.pointer_spring.snap(p);
                self.pointer_initialized = true;
            } else {
                self.pointer_spring.target = p;
            }
        } else {
            self.pointer_initialized = false;
        }
    }

    pub fn ensure_item(&mut self, key: &str, pop_in: bool) -> &mut ItemAnimState {
        let s = self.smoothness;
        self.items.entry(key.to_string()).or_insert_with(|| {
            let mut st = ItemAnimState::new(if pop_in { 0.0 } else { 1.0 });
            let k_scale = 320.0 / s;
            st.scale.stiffness = k_scale;
            st.scale.damping = 2.0 * k_scale.sqrt() * 0.94;
            st.hover_lift.stiffness = k_scale;
            st.hover_lift.damping = 2.0 * k_scale.sqrt() * 0.90;
            let k_shift = 280.0 / s;
            st.hover_shift.stiffness = k_shift;
            st.hover_shift.damping = 2.0 * k_shift.sqrt() * 0.88;
            let k_ang = 240.0 / s;
            st.hover_angle.stiffness = k_ang;
            st.hover_angle.damping = 2.0 * k_ang.sqrt() * 0.56;
            st
        })
    }

    pub fn retain_keys(&mut self, active_keys: &[String]) {
        self.items.retain(|k, _| active_keys.iter().any(|ak| ak == k));
    }

    pub fn trigger_bounce(&mut self, key: &str) {
        self.ensure_item(key, false).bounce.trigger(4);
    }

    pub fn notify_launched(&mut self, key: &str) {
        if let Some(item) = self.items.get_mut(key) {
            item.bounce.notify_launched();
        }
    }

    pub fn step(&mut self, dt: f64, icon_size: f64) {
        self.hover_spring.step(dt);
        self.reveal_spring.step(dt);
        if self.pointer_initialized {
            self.pointer_spring.step(dt);
        }
        let intensity = self.bounce_intensity;
        let kind = self.launch_animation;
        for item in self.items.values_mut() {
            item.scale.step(dt);
            item.hover_sx.step(dt);
            item.hover_sy.step(dt);
            item.hover_skew.step(dt);
            item.hover_lift.step(dt);
            item.hover_shift.step(dt);
            item.hover_angle.step(dt);
            item.hover_glow.step(dt);
            item.presence.step(dt);
            item.drag_shift.step(dt);
            item.launch_pose = item.bounce.step_pose(dt, icon_size, intensity, kind);
        }
    }

    pub fn is_settled(&self) -> bool {
        self.hover_spring.is_settled()
            && self.reveal_spring.is_settled()
            && (!self.pointer_initialized || self.pointer_spring.is_settled())
            && self.items.values().all(ItemAnimState::is_settled)
    }
}

pub fn should_unpin_drag(perp_distance: f64, icon_size: f64) -> bool {
    perp_distance.abs() > icon_size * 1.8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_unpin_threshold_matches_spec() {
        assert!(!should_unpin_drag(50.0, 52.0));
        assert!(should_unpin_drag(100.0, 52.0));
    }
}
