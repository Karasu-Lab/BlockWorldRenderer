use std::collections::BTreeMap;

use bwr_resourcepack::ResourcePackError;
use bwr_serializer::{SerializerError, Value};
use thiserror::Error;

#[cfg(feature = "cli")]
pub mod cli;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DownloaderError {
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    Checksum { expected: String, actual: String },
    #[error(transparent)]
    Serializer(#[from] SerializerError),
    #[error(transparent)]
    Pack(#[from] ResourcePackError),
}

impl DownloaderError {
    pub fn code(&self) -> u16 {
        match self {
            DownloaderError::Http(_) => 1,
            DownloaderError::Checksum { .. } => 2,
            DownloaderError::Serializer(_) => 3,
            DownloaderError::Pack(_) => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionManifest {
    pub latest: String,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetIndex {
    pub objects: BTreeMap<String, AssetObject>,
}

fn into_fields(value: Value) -> Result<BTreeMap<String, Value>, SerializerError> {
    match value {
        Value::Map(fields) => Ok(fields),
        other => Err(SerializerError::TypeMismatch {
            expected: "map",
            found: other.kind(),
        }),
    }
}

fn take_field(fields: &mut BTreeMap<String, Value>, name: &str) -> Result<Value, SerializerError> {
    fields
        .remove(name)
        .ok_or_else(|| SerializerError::MissingField(name.to_owned()))
}

impl TryFrom<Value> for VersionEntry {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let mut fields = into_fields(value)?;
        Ok(Self {
            id: String::try_from(take_field(&mut fields, "id")?)?,
            url: String::try_from(take_field(&mut fields, "url")?)?,
        })
    }
}

impl TryFrom<Value> for VersionManifest {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let mut fields = into_fields(value)?;
        Ok(Self {
            latest: String::try_from(take_field(&mut fields, "latest")?)?,
            versions: Vec::try_from(take_field(&mut fields, "versions")?)?,
        })
    }
}

impl TryFrom<Value> for AssetObject {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let mut fields = into_fields(value)?;
        let size = i64::try_from(take_field(&mut fields, "size")?)?;
        Ok(Self {
            hash: String::try_from(take_field(&mut fields, "hash")?)?,
            size: u64::try_from(size).map_err(|e| SerializerError::Format(e.to_string()))?,
        })
    }
}

impl TryFrom<Value> for AssetIndex {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let mut fields = into_fields(value)?;
        let objects = into_fields(take_field(&mut fields, "objects")?)?
            .into_iter()
            .map(|(name, object)| AssetObject::try_from(object).map(|object| (name, object)))
            .collect::<Result<_, _>>()?;
        Ok(Self { objects })
    }
}

pub trait HttpClient {
    fn get(&self, url: &str) -> Result<Vec<u8>, DownloaderError>;
}

pub trait Downloader {
    fn versions(&self) -> Result<VersionManifest, DownloaderError>;

    fn asset_index(&self, version: &str) -> Result<AssetIndex, DownloaderError>;

    fn fetch_asset(&self, object: &AssetObject) -> Result<Vec<u8>, DownloaderError>;
}

pub trait PackInstaller {
    fn install(&mut self, version: &str, index: &AssetIndex) -> Result<(), DownloaderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(hash: &str, size: i64) -> Value {
        Value::Map(BTreeMap::from([
            ("hash".to_owned(), Value::from(hash)),
            ("size".to_owned(), Value::from(size)),
        ]))
    }

    #[test]
    fn parses_an_asset_index() {
        let value = Value::Map(BTreeMap::from([(
            "objects".to_owned(),
            Value::Map(BTreeMap::from([(
                "minecraft/sounds.json".to_owned(),
                object("abc", 12),
            )])),
        )]));
        let index = AssetIndex::try_from(value).unwrap();
        assert_eq!(
            index.objects["minecraft/sounds.json"],
            AssetObject {
                hash: "abc".to_owned(),
                size: 12
            }
        );
    }

    #[test]
    fn rejects_a_negative_size() {
        assert!(AssetObject::try_from(object("abc", -1)).is_err());
    }

    #[test]
    fn error_codes_are_unique() {
        let codes = [
            DownloaderError::Http(String::new()).code(),
            DownloaderError::Checksum {
                expected: String::new(),
                actual: String::new(),
            }
            .code(),
            DownloaderError::from(SerializerError::Format(String::new())).code(),
            DownloaderError::from(ResourcePackError::Invalid(String::new())).code(),
        ];
        assert_eq!(codes, [1, 2, 3, 4]);
    }
}
