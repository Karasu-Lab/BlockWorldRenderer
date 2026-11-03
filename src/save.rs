//! A world save loaded from a zip of its folder.
//!
//! Opening a save reads `level.dat` (seed, the singleplayer player)
//! and `playerdata/*.dat` (the other players) through the `nbtparser`
//! crate, and indexes which chunks the region files hold. Serving the
//! chunks themselves — translating Anvil sections into chunk packets —
//! is not implemented yet, so [`SaveWorld::chunk_packet`] reports the
//! chunk instead of inventing terrain; the generated world is used
//! only for the registry data the viewer needs.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use nbtparser::{Compound, Tag};
use pomme_client::viewer::steel::SteelWorld;
use pomme_client::viewer::{BiomeClimate, Dimension, Environment, WorldSource};

use crate::entity::EntityInfo;

/// What opening a save found.
pub struct SaveData {
    pub seed: i64,
    pub players: Vec<EntityInfo>,
    present: HashSet<(i32, i32)>,
}

impl SaveData {
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| format!("not a zip archive: {error}"))?;

        let level_dat =
            find_entry(&mut archive, "level.dat").ok_or("the archive has no level.dat")??;
        let level = parse_gzip_nbt(&level_dat)?;
        let data = level
            .root
            .get("Data")
            .and_then(Tag::as_compound)
            .ok_or("level.dat has no Data compound")?;

        let seed = data
            .get("WorldGenSettings")
            .and_then(Tag::as_compound)
            .and_then(|settings| settings.get("seed"))
            .or_else(|| data.get("RandomSeed"))
            .and_then(Tag::as_i64)
            .unwrap_or(0);

        let mut players = Vec::new();
        if let Some(player) = data.get("Player").and_then(Tag::as_compound) {
            players.push(player_entity(player, None));
        }

        let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        for name in &names {
            if is_player_data(name) {
                let bytes = read_entry(&mut archive, name)?;
                if let Ok(document) = parse_gzip_nbt(&bytes) {
                    let uuid = name
                        .rsplit('/')
                        .next()
                        .and_then(|file| file.strip_suffix(".dat"))
                        .map(str::to_owned);
                    players.push(player_entity(&document.root, uuid));
                }
            }
        }

        let mut present = HashSet::new();
        for name in &names {
            if let Some((region_x, region_z)) = region_coords(name) {
                let mut entry = archive
                    .by_name(name)
                    .map_err(|error| format!("cannot open {name}: {error}"))?;
                let mut header = Vec::new();
                entry
                    .by_ref()
                    .take(4096)
                    .read_to_end(&mut header)
                    .map_err(|error| format!("cannot read {name}: {error}"))?;
                for (index, slot) in header.chunks_exact(4).enumerate() {
                    if slot[..3] != [0, 0, 0] {
                        present.insert((
                            region_x * 32 + (index % 32) as i32,
                            region_z * 32 + (index / 32) as i32,
                        ));
                    }
                }
            }
        }

        Ok(Self {
            seed,
            players,
            present,
        })
    }

    /// The viewer's source for this save, over the generated world's
    /// registry data.
    pub fn world(self, base: SteelWorld) -> SaveWorld {
        SaveWorld {
            base,
            present: self.present,
        }
    }
}

/// The viewer's source for a loaded save.
pub struct SaveWorld {
    base: SteelWorld,
    present: HashSet<(i32, i32)>,
}

impl WorldSource for SaveWorld {
    fn dimension(&self) -> Dimension {
        self.base.dimension()
    }

    fn biome_count(&self) -> usize {
        self.base.biome_count()
    }

    fn biome_climate(&self) -> HashMap<u32, BiomeClimate> {
        self.base.biome_climate()
    }

    fn environment(&self) -> Environment {
        self.base.environment()
    }

    fn chunk_packet(&self, x: i32, z: i32) -> Result<Vec<u8>, String> {
        if self.present.contains(&(x, z)) {
            Err("this save's chunks are not converted for the viewer yet".to_owned())
        } else {
            Err("the save has no chunk here".to_owned())
        }
    }
}

fn is_player_data(name: &str) -> bool {
    name.contains("playerdata/") && name.ends_with(".dat")
}

/// The region coordinates of an overworld region file in the archive.
fn region_coords(name: &str) -> Option<(i32, i32)> {
    if name.contains("DIM-1") || name.contains("DIM1") {
        return None;
    }
    let file = name.rsplit('/').next()?;
    let coords = file.strip_prefix("r.")?.strip_suffix(".mca")?;
    let (x, z) = coords.split_once('.')?;
    Some((x.parse().ok()?, z.parse().ok()?))
}

fn find_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    suffix: &str,
) -> Option<Result<Vec<u8>, String>> {
    let name = archive
        .file_names()
        .filter(|name| name == &suffix || name.ends_with(&format!("/{suffix}")))
        .min_by_key(|name| name.len())
        .map(str::to_owned)?;
    Some(read_entry(archive, &name))
}

fn read_entry(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|error| format!("cannot open {name}: {error}"))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {name}: {error}"))?;
    Ok(bytes)
}

fn parse_gzip_nbt(bytes: &[u8]) -> Result<nbtparser::Document, String> {
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut plain)
        .map_err(|error| format!("not gzip data: {error}"))?;
    nbtparser::parse(&plain).map_err(|error| error.to_string())
}

/// A player entity from its NBT: `Pos`, `Rotation` and the UUID, which
/// modern saves store as an int array and older ones in the file name.
fn player_entity(nbt: &Compound, uuid_fallback: Option<String>) -> EntityInfo {
    let position = nbt.get("Pos").and_then(Tag::as_list).and_then(|list| {
        let mut values = list.iter().map(Tag::as_f64);
        Some([values.next()??, values.next()??, values.next()??])
    });
    let (mut yaw, mut pitch) = (0.0, 0.0);
    if let Some(rotation) = nbt.get("Rotation").and_then(Tag::as_list) {
        if let Some(value) = rotation.first().and_then(Tag::as_f64) {
            yaw = value as f32;
        }
        if let Some(value) = rotation.get(1).and_then(Tag::as_f64) {
            pitch = value as f32;
        }
    }
    let uuid = nbt
        .get("UUID")
        .and_then(|tag| match tag {
            Tag::IntArray(values) if values.len() == 4 => Some(uuid_from_ints(values)),
            _ => None,
        })
        .or(uuid_fallback);
    EntityInfo {
        id: None,
        uuid,
        name: None,
        is_player: true,
        position,
        yaw_deg: yaw,
        pitch_deg: pitch,
    }
}

fn uuid_from_ints(values: &[i32]) -> String {
    let mut bytes = Vec::with_capacity(16);
    for value in values {
        bytes.extend(value.to_be_bytes());
    }
    format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[0..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..16])
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
