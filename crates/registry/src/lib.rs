use std::fmt;
use std::str::FromStr;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceKey {
    namespace: String,
    path: String,
}

impl ResourceKey {
    pub fn new(namespace: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            path: path.into(),
        }
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl FromStr for ResourceKey {
    type Err = RegistryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once(':') {
            Some((namespace, path)) if !namespace.is_empty() && !path.is_empty() => {
                Ok(Self::new(namespace, path))
            }
            None if !s.is_empty() => Ok(Self::new("minecraft", s)),
            _ => Err(RegistryError::InvalidKey(s.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegistryId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistryError {
    #[error("invalid resource key `{0}`")]
    InvalidKey(String),
    #[error("duplicate resource key `{0}`")]
    DuplicateKey(ResourceKey),
}

impl RegistryError {
    pub fn code(&self) -> u16 {
        match self {
            RegistryError::InvalidKey(_) => 1,
            RegistryError::DuplicateKey(_) => 2,
        }
    }
}

pub trait Registry<T> {
    fn get(&self, key: &ResourceKey) -> Option<&T>;
    fn get_by_id(&self, id: RegistryId) -> Option<&T>;
    fn id_of(&self, key: &ResourceKey) -> Option<RegistryId>;
    fn register(&mut self, key: ResourceKey, value: T) -> Result<RegistryId, RegistryError>;
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_namespaced_key() {
        let key: ResourceKey = "example:stone".parse().unwrap();
        assert_eq!(key.namespace(), "example");
        assert_eq!(key.path(), "stone");
        assert_eq!(key.to_string(), "example:stone");
    }

    #[test]
    fn defaults_to_the_minecraft_namespace() {
        let key: ResourceKey = "stone".parse().unwrap();
        assert_eq!(key, ResourceKey::new("minecraft", "stone"));
    }

    #[test]
    fn rejects_an_empty_key() {
        assert!("".parse::<ResourceKey>().is_err());
        assert!(":stone".parse::<ResourceKey>().is_err());
    }

    #[test]
    fn error_codes_are_unique() {
        let a = RegistryError::InvalidKey(String::new()).code();
        let b = RegistryError::DuplicateKey(ResourceKey::new("a", "b")).code();
        assert_ne!(a, b);
    }
}
