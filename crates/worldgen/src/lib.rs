use bwr_block::{BlockSetter, BlockState};
use bwr_random::RandomSource;
use bwr_registry::ResourceKey;
use bwr_seed::WorldSeed;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorldgenError {
    #[error("invalid world generation definition: {0}")]
    Definition(String),
    #[error("world generation failed: {0}")]
    Generation(String),
}

impl WorldgenError {
    pub fn code(&self) -> u16 {
        match self {
            WorldgenError::Definition(_) => 1,
            WorldgenError::Generation(_) => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedChunk {
    pub x: i32,
    pub z: i32,
    pub states: Vec<BlockState>,
}

pub trait ChunkGenerator {
    fn generate_chunk(
        &self,
        seed: WorldSeed,
        x: i32,
        z: i32,
    ) -> Result<GeneratedChunk, WorldgenError>;
}

pub trait NoiseSampler {
    fn sample(&self, x: f64, y: f64, z: f64) -> f64;
}

pub trait Feature {
    fn key(&self) -> &ResourceKey;

    fn place(&self, random: &mut dyn RandomSource, target: &mut dyn BlockSetter) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let a = WorldgenError::Definition(String::new()).code();
        let b = WorldgenError::Generation(String::new()).code();
        assert_ne!(a, b);
    }
}
