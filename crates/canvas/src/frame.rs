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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_has_no_delta() {
        assert_eq!(FrameClock::default().tick(1234.0), 0.0);
    }

    #[test]
    fn later_frames_report_elapsed_seconds() {
        let mut clock = FrameClock::default();
        clock.tick(1000.0);
        assert!((clock.tick(1016.0) - 0.016).abs() < 1e-6);
    }

    #[test]
    fn stalls_are_clamped() {
        let mut clock = FrameClock::default();
        clock.tick(0.0);
        assert_eq!(clock.tick(60_000.0), FrameClock::MAX_DELTA_SECONDS);
    }

    #[test]
    fn time_never_runs_backwards() {
        let mut clock = FrameClock::default();
        clock.tick(500.0);
        assert_eq!(clock.tick(400.0), 0.0);
    }
}
