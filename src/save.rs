//! A world save loaded from a zip of its folder.
//!
//! Opening a save reads `level.dat` (seed, the singleplayer player)
//! and `playerdata/*.dat` (the other players) through the `nbtparser`
//! crate, and lifts every chunk payload out of the region files.
//! Serving a chunk translates its Anvil sections — palettes of block
//! state names resolved through the registry, packed data, heightmaps
//! and light layers — into the chunk packet the viewer reads, so the
//! terrain shown is the save's own. The generated world is used only
//! for the registry data the viewer needs.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::{Mutex, PoisonError};

use nbtparser::{Compound, Tag};
use pomme_client::viewer::steel::SteelWorld;
use pomme_client::viewer::{BiomeClimate, Dimension, Environment, WorldSource};
use pomme_protocol::section::Section;
use steel_registry::{REGISTRY, RegistryExt};
use steel_utils::Identifier;

use crate::chunkdata::{
    ChunkData, LIGHT_BYTES, biome_count, block_state_count, fill_biomes, is_fluid,
};
use crate::entity::EntityInfo;

/// One chunk's payload as its region file stores it: the compression
/// id and the still-compressed bytes.
struct StoredChunk {
    compression: u8,
    data: Vec<u8>,
}

/// What opening a save found.
pub struct SaveData {
    pub seed: i64,
    pub players: Vec<EntityInfo>,
    chunks: HashMap<(i32, i32), StoredChunk>,
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

        let mut chunks = HashMap::new();
        for name in &names {
            let Some((region_x, region_z)) = region_coords(name) else {
                continue;
            };
            let bytes = read_entry(&mut archive, name)?;
            if bytes.len() < 4096 {
                continue;
            }
            for (index, slot) in bytes[..4096].chunks_exact(4).enumerate() {
                if slot[3] == 0 {
                    continue;
                }
                let offset =
                    (usize::from(slot[0]) << 16 | usize::from(slot[1]) << 8 | usize::from(slot[2]))
                        * 4096;
                let Some(header) = bytes.get(offset..offset + 5) else {
                    continue;
                };
                let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
                if length == 0 {
                    continue;
                }
                let Some(data) = bytes.get(offset + 5..offset + 4 + length) else {
                    continue;
                };
                chunks.insert(
                    (
                        region_x * 32 + (index % 32) as i32,
                        region_z * 32 + (index / 32) as i32,
                    ),
                    StoredChunk {
                        compression: header[4],
                        data: data.to_vec(),
                    },
                );
            }
        }

        Ok(Self {
            seed,
            players,
            chunks,
        })
    }

    /// The viewer's source for this save, over the generated world's
    /// registry data.
    pub fn world(self, base: SteelWorld) -> SaveWorld {
        SaveWorld {
            base,
            chunks: self.chunks,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

/// The viewer's source for a loaded save.
pub struct SaveWorld {
    base: SteelWorld,
    chunks: HashMap<(i32, i32), StoredChunk>,
    cache: Mutex<HashMap<(i32, i32), Vec<u8>>>,
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
        if let Some(packet) = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(x, z))
        {
            return Ok(packet.clone());
        }
        let Some(stored) = self.chunks.get(&(x, z)) else {
            return Err("the save has no chunk here".to_owned());
        };
        let packet = convert_chunk(stored, x, z, &self.base.dimension())?;
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((x, z), packet.clone());
        Ok(packet)
    }
}

/// Translates one stored chunk into the packet body for its column.
fn convert_chunk(
    stored: &StoredChunk,
    x: i32,
    z: i32,
    dimension: &Dimension,
) -> Result<Vec<u8>, String> {
    let plain = decompress(stored)?;
    let document = nbtparser::parse(&plain).map_err(|error| error.to_string())?;
    let root = &document.root;
    let version = root.get("DataVersion").and_then(Tag::as_i32).unwrap_or(0);
    if version < 2844 {
        return Err(format!(
            "this save's chunks use DataVersion {version}, which predates the current chunk format"
        ));
    }
    let section_count = (dimension.height / 16) as usize;
    let min_section_y = dimension.min_y / 16;
    let plains = biome_id("minecraft:plains")?;
    let mut chunk = ChunkData::empty(section_count);
    for section in &mut chunk.sections {
        fill_biomes(section, plains);
    }
    if let Some(heightmaps) = root.get("Heightmaps").and_then(Tag::as_compound) {
        for (name, id) in [
            ("WORLD_SURFACE", 1),
            ("MOTION_BLOCKING", 4),
            ("MOTION_BLOCKING_NO_LEAVES", 5),
        ] {
            if let Some(Tag::LongArray(values)) = heightmaps.get(name) {
                chunk.heightmaps.push((
                    id,
                    values
                        .iter()
                        .map(|value| *value as u64)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ));
            }
        }
    }
    let sections = root
        .get("sections")
        .and_then(Tag::as_list)
        .ok_or("the chunk has no sections")?;
    for entry in sections {
        let Some(compound) = entry.as_compound() else {
            continue;
        };
        let Some(y) = compound.get("Y").and_then(Tag::as_i32) else {
            continue;
        };
        let Ok(index) = usize::try_from(y - min_section_y) else {
            continue;
        };
        if index >= section_count {
            continue;
        }
        chunk.sections[index] = convert_section(compound, plains)?;
        if let Some(Tag::ByteArray(bytes)) = compound.get("BlockLight") {
            chunk.block_light[index] = light_layer(bytes);
        }
        if let Some(Tag::ByteArray(bytes)) = compound.get("SkyLight") {
            chunk.sky_light[index] = light_layer(bytes);
        }
    }
    Ok(chunk.encode(x, z, dimension.has_skylight))
}

/// Translates one Anvil section: its block-state palette and packed
/// data become registry ids in a section, its biomes likewise.
fn convert_section(compound: &Compound, plains: u32) -> Result<Section, String> {
    let mut section = Section::default();
    let states = compound.get("block_states").and_then(Tag::as_compound);
    let palette = states
        .and_then(|states| states.get("palette"))
        .and_then(Tag::as_list)
        .unwrap_or(&[]);
    let mut ids = Vec::with_capacity(palette.len());
    for entry in palette {
        ids.push(block_state_id(entry)?);
    }
    let data: &[i64] = match states.and_then(|states| states.get("data")) {
        Some(Tag::LongArray(values)) => values,
        _ => &[],
    };
    let (mut blocks, mut fluids) = (0, 0);
    if ids.len() == 1 {
        let state = ids[0];
        if state != 0 {
            for y in 0..16 {
                for z in 0..16 {
                    for x in 0..16 {
                        section
                            .states
                            .get_and_set(x, y, z, state, block_state_count());
                    }
                }
            }
            blocks = 4096;
            if is_fluid(state) {
                fluids = 4096;
            }
        }
    } else if ids.len() > 1 {
        let bits = storage_bits(ids.len(), 4);
        for i in 0..4096 {
            let state = ids[unpack(data, i, bits)];
            if state == 0 {
                continue;
            }
            section.states.get_and_set(
                (i % 16) as u32,
                (i / 256) as u32,
                ((i / 16) % 16) as u32,
                state,
                block_state_count(),
            );
            blocks += 1;
            if is_fluid(state) {
                fluids += 1;
            }
        }
    }
    section.block_count = blocks;
    section.fluid_count = fluids;

    let biomes = compound.get("biomes").and_then(Tag::as_compound);
    let palette = biomes
        .and_then(|biomes| biomes.get("palette"))
        .and_then(Tag::as_list)
        .unwrap_or(&[]);
    let mut ids = Vec::with_capacity(palette.len());
    for entry in palette {
        let name = entry
            .as_str()
            .ok_or("a biome palette entry is not a name")?;
        ids.push(biome_id(name)?);
    }
    let data: &[i64] = match biomes.and_then(|biomes| biomes.get("data")) {
        Some(Tag::LongArray(values)) => values,
        _ => &[],
    };
    if ids.len() == 1 {
        fill_biomes(&mut section, ids[0]);
    } else if ids.len() > 1 {
        let bits = storage_bits(ids.len(), 1);
        for i in 0..64 {
            section.biomes.get_and_set(
                (i % 4) as u32,
                (i / 16) as u32,
                ((i / 4) % 4) as u32,
                ids[unpack(data, i, bits)],
                biome_count(),
            );
        }
    } else {
        fill_biomes(&mut section, plains);
    }
    Ok(section)
}

/// A palette entry's registry id, from its name and properties.
fn block_state_id(entry: &Tag) -> Result<u32, String> {
    let entry = entry
        .as_compound()
        .ok_or("a block palette entry is not a compound")?;
    let name = entry
        .get("Name")
        .and_then(Tag::as_str)
        .ok_or("a block palette entry has no name")?;
    let identifier: Identifier = name.parse().map_err(|_| format!("bad block name {name}"))?;
    let mut properties: Vec<(&str, &str)> = Vec::new();
    if let Some(values) = entry.get("Properties").and_then(Tag::as_compound) {
        for (key, value) in values.iter() {
            if let Some(value) = value.as_str() {
                properties.push((key, value));
            }
        }
    }
    REGISTRY
        .blocks
        .state_id_from_properties(&identifier, &properties)
        .map(|state| u32::from(state.0))
        .ok_or_else(|| format!("unknown block state {name}"))
}

/// A biome's registry id, from its name.
fn biome_id(name: &str) -> Result<u32, String> {
    let identifier: Identifier = name.parse().map_err(|_| format!("bad biome name {name}"))?;
    let id = REGISTRY
        .biomes
        .id_from_key(&identifier)
        .ok_or_else(|| format!("unknown biome {name}"))?;
    u32::try_from(id).map_err(|_| format!("biome {name} has an out-of-range id"))
}

/// The bits one packed entry of a palette of `entries` takes in an
/// Anvil storage array: enough to name every entry, at least `minimum`.
fn storage_bits(entries: usize, minimum: u32) -> u32 {
    entries.next_power_of_two().trailing_zeros().max(minimum)
}

/// One entry out of an Anvil packed long array; entries never span
/// two longs, and missing longs read as zeroes.
fn unpack(longs: &[i64], index: usize, bits: u32) -> usize {
    let per_long = 64 / bits as usize;
    let word = longs.get(index / per_long).copied().unwrap_or(0) as u64;
    let shift = (index % per_long) * bits as usize;
    ((word >> shift) & ((1u64 << bits) - 1)) as usize
}

fn light_layer(bytes: &[u8]) -> Option<Box<[u8; LIGHT_BYTES]>> {
    <[u8; LIGHT_BYTES]>::try_from(bytes).ok().map(Box::new)
}

fn decompress(stored: &StoredChunk) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let result = match stored.compression {
        1 => flate2::read::GzDecoder::new(stored.data.as_slice()).read_to_end(&mut out),
        2 => flate2::read::ZlibDecoder::new(stored.data.as_slice()).read_to_end(&mut out),
        3 => Ok({
            out.extend_from_slice(&stored.data);
            out.len()
        }),
        4 => return Err("this chunk is lz4-compressed, which is not supported".to_owned()),
        other if other & 0x80 != 0 => {
            return Err("this chunk is stored in an external .mcc file".to_owned());
        }
        other => return Err(format!("unknown chunk compression {other}")),
    };
    result.map_err(|error| format!("cannot decompress the chunk: {error}"))?;
    Ok(out)
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
