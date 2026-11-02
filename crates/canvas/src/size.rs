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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_css_pixels_by_the_pixel_ratio() {
        let size = CanvasSize::from_css(800.0, 600.0, 2.0, 8192);
        assert_eq!(
            size,
            CanvasSize {
                width: 1600,
                height: 1200
            }
        );
    }

    #[test]
    fn never_collapses_to_zero() {
        let size = CanvasSize::from_css(0.0, -4.0, 1.0, 8192);
        assert_eq!(
            size,
            CanvasSize {
                width: 1,
                height: 1
            }
        );
    }

    #[test]
    fn clamps_to_the_device_limit() {
        let size = CanvasSize::from_css(10_000.0, 100.0, 1.0, 4096);
        assert_eq!(size.width, 4096);
    }

    #[test]
    fn invalid_pixel_ratio_falls_back_to_one() {
        let size = CanvasSize::from_css(100.0, 50.0, f64::NAN, 4096);
        assert_eq!(
            size,
            CanvasSize {
                width: 100,
                height: 50
            }
        );
    }
}
