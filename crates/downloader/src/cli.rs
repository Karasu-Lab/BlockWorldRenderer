use std::io::Write;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use thiserror::Error;

use crate::{Downloader, DownloaderError, PackInstaller};

const BIN_NAME: &str = "downloader";

#[derive(Debug, Parser)]
#[command(name = BIN_NAME, about = "Fetch and apply Mojang resource packs")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    List,
    Download { version: String },
    Apply { version: String },
    Completions { shell: Shell },
}

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Downloader(#[from] DownloaderError),
    #[error("failed to write output: {0}")]
    Output(#[from] std::io::Error),
}

impl CliError {
    pub fn code(&self) -> u16 {
        match self {
            CliError::Downloader(_) => 1,
            CliError::Output(_) => 2,
        }
    }
}

pub fn run<D: Downloader + PackInstaller>(
    downloader: &mut D,
    cli: Cli,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    match cli.command {
        Command::List => {
            for version in downloader.versions()?.versions {
                writeln!(out, "{}", version.id)?;
            }
        }
        Command::Download { version } => {
            let index = downloader.asset_index(&version)?;
            for object in index.objects.values() {
                downloader.fetch_asset(object)?;
            }
            writeln!(out, "downloaded {} assets", index.objects.len())?;
        }
        Command::Apply { version } => {
            let index = downloader.asset_index(&version)?;
            downloader.install(&version, &index)?;
            writeln!(out, "applied {version}")?;
        }
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), BIN_NAME, out);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetIndex, AssetObject, VersionEntry, VersionManifest};

    struct Fake;

    impl Downloader for Fake {
        fn versions(&self) -> Result<VersionManifest, DownloaderError> {
            Ok(VersionManifest {
                latest: "26.3".to_owned(),
                versions: vec![VersionEntry {
                    id: "26.3".to_owned(),
                    url: String::new(),
                }],
            })
        }

        fn asset_index(&self, _version: &str) -> Result<AssetIndex, DownloaderError> {
            Ok(AssetIndex::default())
        }

        fn fetch_asset(&self, _object: &AssetObject) -> Result<Vec<u8>, DownloaderError> {
            Ok(Vec::new())
        }
    }

    impl PackInstaller for Fake {
        fn install(&mut self, _version: &str, _index: &AssetIndex) -> Result<(), DownloaderError> {
            Ok(())
        }
    }

    fn run_with(args: &[&str]) -> String {
        let cli = Cli::try_parse_from(args).unwrap();
        let mut out = Vec::new();
        run(&mut Fake, cli, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn argument_definitions_are_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn lists_versions() {
        assert_eq!(run_with(&[BIN_NAME, "list"]), "26.3\n");
    }

    #[test]
    fn generates_completions_for_each_shell() {
        for shell in ["bash", "zsh", "fish"] {
            let script = run_with(&[BIN_NAME, "completions", shell]);
            assert!(script.contains(BIN_NAME), "{shell} script is empty");
        }
    }

    #[test]
    fn error_codes_are_unique() {
        let a = CliError::from(DownloaderError::Http(String::new())).code();
        let b = CliError::from(std::io::Error::other("x")).code();
        assert_ne!(a, b);
    }
}
