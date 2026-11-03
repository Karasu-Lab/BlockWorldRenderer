//! NBT parsing behind an interface.
//!
//! The tag model and the [`Parser`] trait in this file are the whole
//! surface the rest of the workspace uses. Which community crate does
//! the actual parsing is a feature choice (`simdnbt` today): consumers
//! never name it, and with no backend enabled [`parse`] reports
//! [`Error::NoBackend`] so the capability can be switched off.

use std::fmt;

/// One NBT tag, in this crate's own representation.
#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<Tag>),
    Compound(Compound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub fn as_compound(&self) -> Option<&Compound> {
        match self {
            Tag::Compound(compound) => Some(compound),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Tag]> {
        match self {
            Tag::List(tags) => Some(tags),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Tag::Byte(value) => Some(i32::from(*value)),
            Tag::Short(value) => Some(i32::from(*value)),
            Tag::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Tag::Byte(value) => Some(i64::from(*value)),
            Tag::Short(value) => Some(i64::from(*value)),
            Tag::Int(value) => Some(i64::from(*value)),
            Tag::Long(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Tag::Float(value) => Some(f64::from(*value)),
            Tag::Double(value) => Some(*value),
            _ => None,
        }
    }
}

/// A compound tag: its entries, in document order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Compound(Vec<(String, Tag)>);

impl Compound {
    pub fn get(&self, name: &str) -> Option<&Tag> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, tag)| tag)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Tag)> {
        self.0.iter().map(|(key, tag)| (key.as_str(), tag))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A parsed NBT document: the root compound and its name.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub name: String,
    pub root: Compound,
}

/// Why parsing failed.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The crate was built without a parser backend.
    NoBackend,
    /// The backend rejected the input.
    Parse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoBackend => write!(f, "no NBT parser backend is enabled"),
            Error::Parse(message) => write!(f, "invalid NBT: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// The interface a parsing backend implements.
pub trait Parser {
    /// Parses a complete NBT document (a named root compound).
    fn parse(&self, bytes: &[u8]) -> Result<Document, Error>;
}

/// Parses `bytes` with the backend the build selected.
pub fn parse(bytes: &[u8]) -> Result<Document, Error> {
    #[cfg(feature = "simdnbt")]
    {
        backend::SimdnbtParser.parse(bytes)
    }
    #[cfg(not(feature = "simdnbt"))]
    {
        let _ = bytes;
        Err(Error::NoBackend)
    }
}

#[cfg(feature = "simdnbt")]
mod backend {
    use std::io::Cursor;

    use simdnbt::owned::{Nbt, NbtCompound, NbtList, NbtTag};

    use super::{Compound, Document, Error, Parser, Tag};

    /// The `simdnbt` crate, adapted to this crate's model.
    pub struct SimdnbtParser;

    impl Parser for SimdnbtParser {
        fn parse(&self, bytes: &[u8]) -> Result<Document, Error> {
            let mut cursor = Cursor::new(bytes);
            match simdnbt::owned::read(&mut cursor)
                .map_err(|error| Error::Parse(error.to_string()))?
            {
                Nbt::Some(nbt) => Ok(Document {
                    name: nbt.name().to_string(),
                    root: compound(&nbt),
                }),
                Nbt::None => Err(Error::Parse("the document is empty".into())),
            }
        }
    }

    fn compound(value: &NbtCompound) -> Compound {
        Compound(
            value
                .iter()
                .map(|(name, tag)| (name.to_string(), self::tag(tag)))
                .collect(),
        )
    }

    fn tag(value: &NbtTag) -> Tag {
        match value {
            NbtTag::Byte(value) => Tag::Byte(*value),
            NbtTag::Short(value) => Tag::Short(*value),
            NbtTag::Int(value) => Tag::Int(*value),
            NbtTag::Long(value) => Tag::Long(*value),
            NbtTag::Float(value) => Tag::Float(*value),
            NbtTag::Double(value) => Tag::Double(*value),
            NbtTag::ByteArray(value) => Tag::ByteArray(value.clone()),
            NbtTag::String(value) => Tag::String(value.to_string()),
            NbtTag::List(value) => Tag::List(list(value)),
            NbtTag::Compound(value) => Tag::Compound(compound(value)),
            NbtTag::IntArray(value) => Tag::IntArray(value.clone()),
            NbtTag::LongArray(value) => Tag::LongArray(value.clone()),
        }
    }

    fn list(value: &NbtList) -> Vec<Tag> {
        match value {
            NbtList::Empty => Vec::new(),
            NbtList::Byte(values) => values.iter().map(|v| Tag::Byte(*v)).collect(),
            NbtList::Short(values) => values.iter().map(|v| Tag::Short(*v)).collect(),
            NbtList::Int(values) => values.iter().map(|v| Tag::Int(*v)).collect(),
            NbtList::Long(values) => values.iter().map(|v| Tag::Long(*v)).collect(),
            NbtList::Float(values) => values.iter().map(|v| Tag::Float(*v)).collect(),
            NbtList::Double(values) => values.iter().map(|v| Tag::Double(*v)).collect(),
            NbtList::ByteArray(values) => {
                values.iter().map(|v| Tag::ByteArray(v.clone())).collect()
            }
            NbtList::String(values) => values.iter().map(|v| Tag::String(v.to_string())).collect(),
            NbtList::List(values) => values.iter().map(|v| Tag::List(list(v))).collect(),
            NbtList::Compound(values) => {
                values.iter().map(|v| Tag::Compound(compound(v))).collect()
            }
            NbtList::IntArray(values) => values.iter().map(|v| Tag::IntArray(v.clone())).collect(),
            NbtList::LongArray(values) => {
                values.iter().map(|v| Tag::LongArray(v.clone())).collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal level.dat-shaped document, written out by hand.
    fn sample() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(10); // compound, root name ""
        bytes.extend(0u16.to_be_bytes());
        bytes.push(3); // int "DataVersion"
        bytes.extend(11u16.to_be_bytes());
        bytes.extend(b"DataVersion");
        bytes.extend(4325i32.to_be_bytes());
        bytes.push(8); // string "LevelName"
        bytes.extend(9u16.to_be_bytes());
        bytes.extend(b"LevelName");
        bytes.extend(4u16.to_be_bytes());
        bytes.extend(b"test");
        bytes.push(10); // compound "Data"
        bytes.extend(4u16.to_be_bytes());
        bytes.extend(b"Data");
        bytes.push(4); // long "RandomSeed"
        bytes.extend(10u16.to_be_bytes());
        bytes.extend(b"RandomSeed");
        bytes.extend(42i64.to_be_bytes());
        bytes.push(0);
        bytes.push(0);
        bytes
    }

    #[cfg(feature = "simdnbt")]
    #[test]
    fn parses_a_document() {
        let document = parse(&sample()).expect("the sample parses");
        assert_eq!(document.name, "");
        assert_eq!(
            document.root.get("DataVersion").and_then(Tag::as_i32),
            Some(4325)
        );
        assert_eq!(
            document.root.get("LevelName").and_then(Tag::as_str),
            Some("test")
        );
        let data = document
            .root
            .get("Data")
            .and_then(Tag::as_compound)
            .expect("the nested compound");
        assert_eq!(data.get("RandomSeed").and_then(Tag::as_i64), Some(42));
    }

    #[cfg(feature = "simdnbt")]
    #[test]
    fn rejects_garbage() {
        assert!(matches!(parse(&[1, 2, 3]), Err(Error::Parse(_))));
    }

    #[cfg(not(feature = "simdnbt"))]
    #[test]
    fn reports_the_missing_backend() {
        assert_eq!(parse(&sample()), Err(Error::NoBackend));
    }
}
