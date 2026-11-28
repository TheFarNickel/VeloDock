#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
    pub stiffness: f64,
    pub damping: f64,
}

impl Spring {
    pub fn new(initial: f64, stiffness: f64, damping: f64) -> Self {
        Self {
            value: initial,
            velocity: 0.0,
            target: initial,
            stiffness,
            damping,
        }
    }

    pub fn snap(&mut self, value: f64) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    pub fn step(&mut self, dt: f64) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }

        let dt = dt.min(0.033);
        let force = self.stiffness * (self.target - self.value) - self.damping * self.velocity;
        self.velocity += force * dt;
        self.value += self.velocity * dt;

        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
        }
    }

    pub fn is_settled(&self) -> bool {
        (self.target - self.value).abs() < 0.001 && self.velocity.abs() < 0.001
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_converges_and_survives_lag_spike() {
        let mut s = Spring::new(0.0, 280.0, 26.0);
        s.target = 1.0;
        s.step(5.0);
        assert!(s.value.is_finite() && s.value < 2.0);
        for _ in 0..240 {
            s.step(1.0 / 60.0);
        }
        assert!(s.is_settled());
        assert!((s.value - 1.0).abs() < 0.002);
    }
}
