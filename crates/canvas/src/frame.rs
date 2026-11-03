/// Turns `requestAnimationFrame` timestamps into frame deltas.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameClock {
    last_ms: Option<f64>,
}

impl FrameClock {
    /// A long stall, such as a hidden tab, must not become one huge step.
    pub const MAX_DELTA_SECONDS: f32 = 0.25;

    pub fn tick(&mut self, now_ms: f64) -> f32 {
        let delta = match self.last_ms {
            Some(last) => ((now_ms - last) / 1000.0) as f32,
            None => 0.0,
        };
        self.last_ms = Some(now_ms);
        delta.clamp(0.0, Self::MAX_DELTA_SECONDS)
    }
}
