use std::collections::BTreeMap;

pub use registry::ResourceKey;
use serializer::{SerializerError, Value};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMetadata {
    pub name: String,
    pub format: u32,
}

impl TryFrom<Value> for PackMetadata {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let Value::Map(mut fields) = value else {
            return Err(SerializerError::TypeMismatch {
                expected: "map",
                found: value.kind(),
            });
        };
        let mut take = |name: &str| {
            fields
                .remove(name)
                .ok_or_else(|| SerializerError::MissingField(name.to_owned()))
        };
        let name = String::try_from(take("name")?)?;
        let format = u32::try_from(i64::try_from(take("format")?)?)
            .map_err(|e| SerializerError::Format(e.to_string()))?;
        Ok(Self { name, format })
    }
}

impl From<PackMetadata> for Value {
    fn from(metadata: PackMetadata) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert("name".to_owned(), Value::from(metadata.name));
        fields.insert("format".to_owned(), Value::from(i64::from(metadata.format)));
        Value::Map(fields)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResourcePackError {
    #[error("resource `{0}` was not found")]
    NotFound(ResourceKey),
    #[error("invalid resource pack: {0}")]
    Invalid(String),
}

impl ResourcePackError {
    pub fn code(&self) -> u16 {
        match self {
            ResourcePackError::NotFound(_) => 1,
            ResourcePackError::Invalid(_) => 2,
        }
    }
}

pub trait ResourceProvider {
    fn namespaces(&self) -> Vec<String>;

    fn exists(&self, key: &ResourceKey) -> bool;

    fn open(&self, key: &ResourceKey) -> Result<Vec<u8>, ResourcePackError>;

    fn list(&self, namespace: &str, prefix: &str) -> Vec<ResourceKey>;
}

pub trait ResourcePack: ResourceProvider {
    fn metadata(&self) -> &PackMetadata;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_round_trips_through_value() {
        let metadata = PackMetadata {
            name: "vanilla".to_owned(),
            format: 34,
        };
        let value = Value::from(metadata.clone());
        assert_eq!(PackMetadata::try_from(value), Ok(metadata));
    }

    #[test]
    fn metadata_reports_a_missing_field() {
        let value = Value::Map(BTreeMap::new());
        assert_eq!(
            PackMetadata::try_from(value),
            Err(SerializerError::MissingField("name".to_owned()))
        );
    }

    #[test]
    fn error_codes_are_unique() {
        let a = ResourcePackError::NotFound(ResourceKey::new("a", "b")).code();
        let b = ResourcePackError::Invalid(String::new()).code();
        assert_ne!(a, b);
    }
}
