#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasSize {
    pub width: u32,
    pub height: u32,
}

impl CanvasSize {
    /// The backing-store size for a canvas laid out at `css_width` by
    /// `css_height`, clamped to what the device can render into.
    pub fn from_css(css_width: f64, css_height: f64, pixel_ratio: f64, max_dimension: u32) -> Self {
        let ratio = if pixel_ratio.is_finite() && pixel_ratio > 0.0 {
            pixel_ratio
        } else {
            1.0
        };
        let scale = |css: f64| {
            let pixels = (css * ratio).round();
            if pixels.is_finite() {
                (pixels as u32).clamp(1, max_dimension.max(1))
            } else {
                1
            }
        };
        Self {
            width: scale(css_width),
            height: scale(css_height),
        }
    }

    pub fn aspect_ratio(self) -> f32 {
        self.width as f32 / self.height.max(1) as f32
    }
}
