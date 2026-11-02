use bwr_resourcepack::{ResourceKey, ResourcePackError, ResourceProvider};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderSource {
    pub stage: ShaderStage,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderProgram {
    pub name: ResourceKey,
    pub sources: Vec<ShaderSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShaderError {
    #[error("shader `{0}` was not found")]
    NotFound(ResourceKey),
    #[error("shader failed to compile: {0}")]
    Compile(String),
    #[error(transparent)]
    Resource(#[from] ResourcePackError),
}

impl ShaderError {
    pub fn code(&self) -> u16 {
        match self {
            ShaderError::NotFound(_) => 1,
            ShaderError::Compile(_) => 2,
            ShaderError::Resource(_) => 3,
        }
    }
}

pub trait ShaderLoader {
    fn load_program(
        &self,
        provider: &dyn ResourceProvider,
        name: &ResourceKey,
    ) -> Result<ShaderProgram, ShaderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let key = ResourceKey::new("a", "b");
        let codes = [
            ShaderError::NotFound(key.clone()).code(),
            ShaderError::Compile(String::new()).code(),
            ShaderError::from(ResourcePackError::NotFound(key)).code(),
        ];
        assert_eq!(codes, [1, 2, 3]);
    }
}
