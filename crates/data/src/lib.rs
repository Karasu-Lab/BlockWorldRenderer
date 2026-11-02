use bwr_datacomponent::DataComponentMap;
use bwr_registry::ResourceKey;
use bwr_resourcepack::ResourcePackError;
use bwr_serializer::{SerializerError, Value};
use bwr_worldgen::{ChunkGenerator, WorldgenError};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DataError {
    #[error("invalid definition: {0}")]
    InvalidDefinition(String),
    #[error("`{0}` could not be resolved")]
    Unresolved(ResourceKey),
    #[error(transparent)]
    Serializer(#[from] SerializerError),
    #[error(transparent)]
    Pack(#[from] ResourcePackError),
    #[error(transparent)]
    Worldgen(#[from] WorldgenError),
}

impl DataError {
    pub fn code(&self) -> u16 {
        match self {
            DataError::InvalidDefinition(_) => 1,
            DataError::Unresolved(_) => 2,
            DataError::Serializer(_) => 3,
            DataError::Pack(_) => 4,
            DataError::Worldgen(_) => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentType {
    Boolean,
    Integer,
    Float,
    Text,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandDefinition {
    pub name: String,
    pub arguments: Vec<(String, ArgumentType)>,
    pub run: Value,
}

pub trait CommandSource {
    fn has_permission(&self, level: u8) -> bool;

    fn reply(&mut self, message: String);
}

pub trait CommandLoader {
    fn load_command(&self, value: Value) -> Result<CommandDefinition, DataError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAsset {
    pub key: ResourceKey,
    pub pack: String,
    pub bytes: Vec<u8>,
}

pub trait PackResolver {
    fn resolve(&self, key: &ResourceKey) -> Result<ResolvedAsset, DataError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldgenDefinition {
    pub key: ResourceKey,
    pub settings: Value,
    pub components: DataComponentMap,
}

pub trait GeneratorLoader {
    fn load_generator(
        &self,
        definition: &WorldgenDefinition,
    ) -> Result<Box<dyn ChunkGenerator>, DataError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let key = ResourceKey::new("a", "b");
        let codes = [
            DataError::InvalidDefinition(String::new()).code(),
            DataError::Unresolved(key).code(),
            DataError::from(SerializerError::Format(String::new())).code(),
            DataError::from(ResourcePackError::Invalid(String::new())).code(),
            DataError::from(WorldgenError::Definition(String::new())).code(),
        ];
        assert_eq!(codes, [1, 2, 3, 4, 5]);
    }
}
