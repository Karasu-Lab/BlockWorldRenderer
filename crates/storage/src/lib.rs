use registry::ResourceKey;
use serializer::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StorageError {
    #[error("storage I/O failed: {0}")]
    Io(String),
    #[error("stored data is corrupt: {0}")]
    Corrupt(String),
}

impl StorageError {
    pub fn code(&self) -> u16 {
        match self {
            StorageError::Io(_) => 1,
            StorageError::Corrupt(_) => 2,
        }
    }
}

pub trait ChunkStorage {
    fn read_chunk(
        &self,
        dimension: &ResourceKey,
        pos: ChunkPos,
    ) -> Result<Option<Value>, StorageError>;

    fn write_chunk(
        &mut self,
        dimension: &ResourceKey,
        pos: ChunkPos,
        data: Value,
    ) -> Result<(), StorageError>;
}

pub trait WorldStorage {
    fn read_level_data(&self) -> Result<Value, StorageError>;

    fn write_level_data(&mut self, data: Value) -> Result<(), StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let a = StorageError::Io(String::new()).code();
        let b = StorageError::Corrupt(String::new()).code();
        assert_ne!(a, b);
    }
}
