use std::path::{Path, PathBuf};

use crate::ResourcePackError;

pub const DEFAULT_PACK_DIR: &str = "resourcepacks";

/// Where the resource packs live.
///
/// On a desktop that is a directory. In the browser there are no directories,
/// so the same setting names either a namespace of the browser's storage or
/// the base URL packs are fetched from; both are mirrored into a directory of
/// the virtual file system, which [`PackLocation::resolve`] returns.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PackLocation {
    /// `resourcepacks` inside the game directory.
    #[default]
    Default,
    /// A directory; a relative one is taken from the game directory.
    Directory(PathBuf),
    /// A namespace of the browser's storage.
    Namespace(String),
    /// A base URL packs are fetched from.
    Url(String),
}

impl PackLocation {
    pub fn resolve(&self, game_dir: &Path) -> PathBuf {
        let default = game_dir.join(DEFAULT_PACK_DIR);
        match self {
            PackLocation::Default => default,
            PackLocation::Directory(dir) if dir.is_absolute() => dir.clone(),
            PackLocation::Directory(dir) => game_dir.join(dir),
            PackLocation::Namespace(namespace) => default.join(sanitize(namespace)),
            PackLocation::Url(url) => default.join(".remote").join(sanitize(url)),
        }
    }

    /// The browser storage namespace or base URL, if packs come from one.
    pub fn remote(&self) -> Option<&str> {
        match self {
            PackLocation::Namespace(name) | PackLocation::Url(name) => Some(name),
            PackLocation::Default | PackLocation::Directory(_) => None,
        }
    }

    /// The text a settings file or a command line holds: `default`,
    /// `dir:<path>`, `namespace:<name>` or `url:<base>`.
    pub fn to_setting(&self) -> String {
        match self {
            PackLocation::Default => "default".to_owned(),
            PackLocation::Directory(dir) => format!("dir:{}", dir.display()),
            PackLocation::Namespace(name) => format!("namespace:{name}"),
            PackLocation::Url(base) => format!("url:{base}"),
        }
    }

    /// The location [`PackLocation::to_setting`] wrote. An empty text is the
    /// default.
    pub fn parse(text: &str) -> Result<Self, ResourcePackError> {
        let text = text.trim();
        if text.is_empty() || text == "default" {
            return Ok(PackLocation::Default);
        }
        let invalid =
            |reason: &str| ResourcePackError::Invalid(format!("pack location `{text}`: {reason}"));
        let (kind, value) = text
            .split_once(':')
            .ok_or_else(|| invalid("expected `dir:`, `namespace:` or `url:`"))?;
        match kind {
            "dir" if !value.is_empty() => Ok(PackLocation::Directory(PathBuf::from(value))),
            "namespace" if !value.is_empty() => Ok(PackLocation::Namespace(value.to_owned())),
            "url"
                if value.starts_with("http://")
                    || value.starts_with("https://")
                    || value.starts_with('/') =>
            {
                Ok(PackLocation::Url(value.trim_end_matches('/').to_owned()))
            }
            "dir" | "namespace" => Err(invalid("the value is empty")),
            "url" => Err(invalid("a URL starts with http://, https:// or /")),
            _ => Err(invalid("expected `dir:`, `namespace:` or `url:`")),
        }
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
