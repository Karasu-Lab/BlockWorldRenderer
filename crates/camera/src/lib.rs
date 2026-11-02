#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 70.0_f32.to_radians(),
            near: 0.05,
            far: 1024.0,
        }
    }
}

pub trait CameraController {
    fn update(&mut self, camera: &mut Camera, delta_seconds: f32);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Spin;

    impl CameraController for Spin {
        fn update(&mut self, camera: &mut Camera, delta_seconds: f32) {
            camera.yaw += delta_seconds;
        }
    }

    #[test]
    fn controller_updates_the_camera() {
        let mut camera = Camera::default();
        Spin.update(&mut camera, 0.5);
        assert_eq!(camera.yaw, 0.5);
    }
}
