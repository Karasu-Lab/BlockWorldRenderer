use camera::Camera;
use seed::WorldSeed;
use thiserror::Error;
use utils::side::Sided;

pub const DEFAULT_SEED: WorldSeed = WorldSeed(8_675_309);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsError {
    #[error("invalid setting `{name}`: {reason}")]
    Invalid { name: &'static str, reason: String },
}

impl SettingsError {
    pub fn code(&self) -> u16 {
        match self {
            SettingsError::Invalid { .. } => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientSettings {
    pub ui_enabled: bool,
    pub debug_screen: bool,
    pub render_distance: u32,
    pub vsync: bool,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            ui_enabled: false,
            debug_screen: false,
            render_distance: 12,
            vsync: true,
        }
    }
}

impl ClientSettings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.render_distance == 0 {
            return Err(SettingsError::Invalid {
                name: "render_distance",
                reason: "must be at least 1".into(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSettings {
    pub tick_rate: u32,
    pub view_distance: u32,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            tick_rate: 20,
            view_distance: 12,
        }
    }
}

impl ServerSettings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.tick_rate == 0 {
            return Err(SettingsError::Invalid {
                name: "tick_rate",
                reason: "must be at least 1".into(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraSettings {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            position: [0.0, 90.0, 0.0],
            yaw: 0.0,
            pitch: -20.0_f32.to_radians(),
        }
    }
}

impl CameraSettings {
    pub fn to_camera(&self) -> Camera {
        Camera {
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            ..Camera::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSettings {
    pub seed: WorldSeed,
}

impl Default for WorldSettings {
    fn default() -> Self {
        Self { seed: DEFAULT_SEED }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Settings {
    pub client: Option<ClientSettings>,
    pub server: Option<ServerSettings>,
    pub camera: CameraSettings,
    pub world: WorldSettings,
}

impl Sided for Settings {
    type Client = ClientSettings;
    type Server = ServerSettings;

    fn client(&self) -> Option<&ClientSettings> {
        self.client.as_ref()
    }

    fn server(&self) -> Option<&ServerSettings> {
        self.server.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_settings_build_a_camera() {
        let camera = CameraSettings::default().to_camera();
        assert_eq!(camera.position, [0.0, 90.0, 0.0]);
    }

    #[test]
    fn default_client_hides_the_ui() {
        assert!(!ClientSettings::default().ui_enabled);
    }

    #[test]
    fn zero_render_distance_is_invalid() {
        let settings = ClientSettings {
            render_distance: 0,
            ..ClientSettings::default()
        };
        assert_eq!(settings.validate().unwrap_err().code(), 1);
    }
}
