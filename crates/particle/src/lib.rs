use camera::Camera;
use thiserror::Error;
use tick::Tickable;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParticleKind(pub String);

#[derive(Debug, Clone, PartialEq)]
pub struct Particle {
    pub kind: ParticleKind,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub age: u32,
    pub lifetime: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParticleError {
    #[error("unknown particle kind: {0}")]
    UnknownKind(String),
    #[error("particle limit reached")]
    LimitReached,
}

impl ParticleError {
    pub fn code(&self) -> u16 {
        match self {
            ParticleError::UnknownKind(_) => 1,
            ParticleError::LimitReached => 2,
        }
    }
}

pub trait ParticleEmitter {
    fn emit(&mut self, particle: Particle) -> Result<(), ParticleError>;
}

pub trait ParticleSystem: ParticleEmitter + Tickable {
    fn particles(&self) -> &[Particle];

    fn visible(&self, camera: &Camera) -> Vec<usize>;

    fn clear(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let codes = [
            ParticleError::UnknownKind(String::new()).code(),
            ParticleError::LimitReached.code(),
        ];
        assert_eq!(codes, [1, 2]);
    }
}
