use camera::Camera;
use resourcepack::PackLocation;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSettings {
    pub ui_enabled: bool,
    pub debug_screen: bool,
    pub render_distance: u32,
    pub vsync: bool,
    pub resource_pack_location: PackLocation,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            ui_enabled: false,
            debug_screen: false,
            render_distance: 12,
            vsync: true,
            resource_pack_location: PackLocation::default(),
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

#[derive(Debug, Clone, PartialEq, Default)]
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
