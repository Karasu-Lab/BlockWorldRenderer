use std::path::{Path, PathBuf};

use thiserror::Error;

#[cfg(target_arch = "wasm32")]
mod page;

#[cfg(target_arch = "wasm32")]
pub use page::{CacheNotifier, listen, load, load_paths, save};

pub const VIEWER_ASSETS: &str = "viewer-assets";
pub const VIEWER_VERSIONS: &str = "viewer-versions";

const NAME_PREFIX: &str = "pomme-vfs:";
const KEY_PREFIX: &str = "https://pomme-vfs.invalid/";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CacheError {
    #[error("the page could not load or save the cache: {0}")]
    Page(String),
    #[error("the page answered with something other than cache entries: {0}")]
    Malformed(String),
}

impl CacheError {
    pub fn code(&self) -> u16 {
        match self {
            CacheError::Page(_) => 1,
            CacheError::Malformed(_) => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

pub fn cache_name(namespace: &str) -> String {
    format!("{NAME_PREFIX}{namespace}")
}

pub fn packs_namespace(base: &str) -> String {
    format!("packs:{base}")
}

pub fn key(path: &Path) -> String {
    let mut url = String::from(KEY_PREFIX);
    for (i, part) in path.components().enumerate() {
        if i > 0 {
            url.push('/');
        }
        encode_component(&part.as_os_str().to_string_lossy(), &mut url);
    }
    url
}

pub fn path_of(key: &str) -> Option<PathBuf> {
    let rest = key.strip_prefix(KEY_PREFIX)?;
    let mut path = PathBuf::new();
    for part in rest.split('/') {
        path.push(decode_component(part)?);
    }
    Some(path)
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

fn encode_component(part: &str, out: &mut String) {
    for &byte in part.as_bytes() {
        if is_unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
}

fn decode_component(part: &str) -> Option<String> {
    let bytes = part.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = part.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_encode_each_component_like_encode_uri_component() {
        let path = Path::new("viewer/assets/objects/ab/a b+c%");
        assert_eq!(
            key(path),
            "https://pomme-vfs.invalid/viewer/assets/objects/ab/a%20b%2Bc%25"
        );
        assert_eq!(
            key(Path::new("/data/é")),
            "https://pomme-vfs.invalid/%2F/data/%C3%A9"
        );
    }

    #[test]
    fn keys_round_trip_to_their_paths() {
        for path in ["/data/assets/indexes/26.2.json", "a/b (1)/c~d", "é/ü"] {
            assert_eq!(path_of(&key(Path::new(path))), Some(PathBuf::from(path)));
        }
    }

    #[test]
    fn foreign_and_broken_keys_have_no_path() {
        assert_eq!(path_of("https://example.com/a"), None);
        assert_eq!(path_of("https://pomme-vfs.invalid/a%2"), None);
        assert_eq!(path_of("https://pomme-vfs.invalid/a%FF"), None);
    }

    #[test]
    fn names_carry_the_namespace() {
        assert_eq!(cache_name(VIEWER_ASSETS), "pomme-vfs:viewer-assets");
        assert_eq!(
            cache_name(&packs_namespace("https://x")),
            "pomme-vfs:packs:https://x"
        );
    }
}
