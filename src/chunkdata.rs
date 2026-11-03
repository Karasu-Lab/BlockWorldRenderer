//! A chunk column in the shape the replay and save sources share:
//! pomme sections, heightmaps and per-section light, plus the encoder
//! that turns it into the level-chunk packet body the viewer reads.
//! The packet is written with SteelMC's packet types, so its bytes
//! match what the generated world serves.

use pomme_protocol::chunk::LightData;
use pomme_protocol::codec::Reader;
use pomme_protocol::section::{PalettedContainer, Section, Strategy};
use steel_protocol::packets::game::{
    CLevelChunkWithLight, ChunkPacketData, HeightmapType, Heightmaps, LightUpdatePacketData,
};
use steel_registry::{REGISTRY, RegistryExt};
use steel_utils::BlockStateId;
use steel_utils::codec::BitSet;
use steel_utils::serial::WriteTo;

/// One light layer: 4×16×16 nibbles.
pub const LIGHT_BYTES: usize = 2048;

/// Sky light above the top of the world is always full.
const FULL_SKY: [u8; LIGHT_BYTES] = [0xFF; LIGHT_BYTES];

/// A decoded chunk column.
#[derive(Clone, Default)]
pub struct ChunkData {
    pub sections: Vec<Section>,
    /// Heightmap type id and packed data, as the packet carries them.
    pub heightmaps: Vec<(u32, Box<[u64]>)>,
    /// Per-section light layers; `None` where the source stored none.
    pub sky_light: Vec<Option<Box<[u8; LIGHT_BYTES]>>>,
    pub block_light: Vec<Option<Box<[u8; LIGHT_BYTES]>>>,
}

impl ChunkData {
    pub fn empty(section_count: usize) -> Self {
        Self {
            sections: (0..section_count).map(|_| Section::default()).collect(),
            heightmaps: Vec::new(),
            sky_light: vec![None; section_count],
            block_light: vec![None; section_count],
        }
    }

    /// Sets one block, keeping the section's block and fluid counts.
    /// `x` and `z` are world coordinates, `y` is a world height, and
    /// `min_y` is the column's lowest block y.
    pub fn set_block(&mut self, min_y: i32, x: i32, y: i32, z: i32, state: u32) {
        let index = (y - min_y).div_euclid(16);
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(section) = self.sections.get_mut(index) else {
            return;
        };
        let old = section.states.get_and_set(
            x.rem_euclid(16) as u32,
            (y - min_y).rem_euclid(16) as u32,
            z.rem_euclid(16) as u32,
            state,
            block_state_count(),
        );
        if old == state {
            return;
        }
        section.block_count = adjusted(section.block_count, old != 0, state != 0);
        section.fluid_count = adjusted(section.fluid_count, is_fluid(old), is_fluid(state));
    }

    /// The packet body for this column, in the generated world's format.
    pub fn encode(&self, x: i32, z: i32, has_skylight: bool) -> Vec<u8> {
        let mut data = Vec::new();
        for section in &self.sections {
            section.write(&mut data);
        }
        let heightmaps = Heightmaps {
            heightmaps: self
                .heightmaps
                .iter()
                .filter_map(|(id, values)| {
                    let kind = match id {
                        1 => HeightmapType::WorldSurface,
                        4 => HeightmapType::MotionBlocking,
                        5 => HeightmapType::MotionBlockingNoLeaves,
                        _ => return None,
                    };
                    Some((kind, values.iter().map(|value| *value as i64).collect()))
                })
                .collect(),
        };
        let packet = CLevelChunkWithLight {
            x,
            z,
            chunk_data: ChunkPacketData {
                heightmaps,
                data,
                block_entities: Vec::new(),
            },
            light_data: self.light_data(has_skylight),
        };
        let mut out = Vec::new();
        packet
            .write(&mut out)
            .expect("writing a packet to memory cannot fail");
        out
    }

    /// The light half of the packet, mirroring how SteelMC builds it
    /// from stored layers: a layer with any lit nibble goes out in
    /// full, an all-dark or missing one is marked empty, and the sky
    /// layer above the top of the world is full.
    fn light_data(&self, has_skylight: bool) -> LightUpdatePacketData {
        let count = self.sections.len() + 2;
        let words = count.div_ceil(64);
        let mut sky_y_mask = BitSet(vec![0; words].into_boxed_slice());
        let mut block_y_mask = BitSet(vec![0; words].into_boxed_slice());
        let mut empty_sky_y_mask = BitSet(vec![0; words].into_boxed_slice());
        let mut empty_block_y_mask = BitSet(vec![0; words].into_boxed_slice());
        let mut sky_updates = Vec::new();
        let mut block_updates = Vec::new();
        for index in 0..count {
            let section = index as i32 - 1;
            if has_skylight {
                let layer = if section == self.sections.len() as i32 {
                    Some(&FULL_SKY)
                } else {
                    self.sky_light
                        .get(usize::try_from(section).unwrap_or(usize::MAX))
                        .and_then(|layer| layer.as_deref())
                };
                push_layer(
                    layer,
                    index,
                    &mut sky_y_mask,
                    &mut empty_sky_y_mask,
                    &mut sky_updates,
                );
            }
            let layer = self
                .block_light
                .get(usize::try_from(section).unwrap_or(usize::MAX))
                .and_then(|layer| layer.as_deref());
            push_layer(
                layer,
                index,
                &mut block_y_mask,
                &mut empty_block_y_mask,
                &mut block_updates,
            );
        }
        LightUpdatePacketData {
            sky_y_mask,
            block_y_mask,
            empty_sky_y_mask,
            empty_block_y_mask,
            sky_updates,
            block_updates,
        }
    }
}

fn push_layer(
    layer: Option<&[u8; LIGHT_BYTES]>,
    index: usize,
    mask: &mut BitSet,
    empty_mask: &mut BitSet,
    updates: &mut Vec<Vec<u8>>,
) {
    match layer {
        Some(bytes) if bytes.iter().any(|byte| *byte != 0) => {
            mask.set(index, true);
            updates.push(bytes.to_vec());
        }
        _ => empty_mask.set(index, true),
    }
}

fn adjusted(count: u16, was: bool, is: bool) -> u16 {
    let count = i32::from(count) + i32::from(is) - i32::from(was);
    u16::try_from(count.clamp(0, 4096)).unwrap_or(0)
}

/// How many block states the registry holds; the width of a global
/// block-state palette is derived from it.
pub fn block_state_count() -> usize {
    usize::from(REGISTRY.blocks.next_state_id)
}

/// How many biomes the registry holds.
pub fn biome_count() -> usize {
    REGISTRY.biomes.len()
}

/// Whether a block state is a fluid (water or lava, any level).
pub fn is_fluid(state: u32) -> bool {
    let Ok(state) = u16::try_from(state) else {
        return false;
    };
    REGISTRY
        .blocks
        .by_state_id(BlockStateId(state))
        .is_some_and(|block| {
            block.key.namespace == "minecraft"
                && (block.key.path == "water" || block.key.path == "lava")
        })
}

/// Fills a section's biome cells with one biome.
pub fn fill_biomes(section: &mut Section, biome: u32) {
    for y in 0..4 {
        for z in 0..4 {
            for x in 0..4 {
                section.biomes.get_and_set(x, y, z, biome, biome_count());
            }
        }
    }
}

/// Reads the sections of a chunk packet body back into sections.
///
/// # Errors
/// Returns an error if the bytes do not hold exactly `section_count`
/// sections.
pub fn sections_from_bytes(bytes: &[u8], section_count: usize) -> Result<Vec<Section>, String> {
    let mut reader = Reader::new(bytes);
    let mut sections = Vec::with_capacity(section_count);
    for _ in 0..section_count {
        sections.push(
            Section::read(&mut reader, block_state_count(), biome_count())
                .map_err(|error| error.to_string())?,
        );
    }
    reader.ensure_eof().map_err(|error| error.to_string())?;
    Ok(sections)
}

/// Splits a packet's light data into per-section layers; the padding
/// layers below and above the world are dropped.
pub fn light_from_packet(
    light: &LightData,
    section_count: usize,
) -> (
    Vec<Option<Box<[u8; LIGHT_BYTES]>>>,
    Vec<Option<Box<[u8; LIGHT_BYTES]>>>,
) {
    let mut sky = vec![None; section_count];
    let mut block = vec![None; section_count];
    take_layers(&light.sky_y_mask, &light.sky_updates, &mut sky);
    take_layers(&light.block_y_mask, &light.block_updates, &mut block);
    (sky, block)
}

fn take_layers(mask: &[u64], updates: &[Box<[u8]>], out: &mut [Option<Box<[u8; LIGHT_BYTES]>>]) {
    let mut next = 0;
    for index in 0..out.len() + 2 {
        let set = mask
            .get(index / 64)
            .is_some_and(|word| (word >> (index % 64)) & 1 == 1);
        if !set {
            continue;
        }
        let Some(layer) = updates.get(next) else {
            return;
        };
        next += 1;
        if index == 0 {
            continue;
        }
        if let Some(slot) = out.get_mut(index - 1) {
            if let Ok(layer) = <[u8; LIGHT_BYTES]>::try_from(layer.as_ref()) {
                *slot = Some(Box::new(layer));
            }
        }
    }
}

/// Reads one biome container per section out of a chunks-biomes
/// packet body and swaps each section's biomes for it.
///
/// # Errors
/// Returns an error if the bytes do not hold one container per section.
pub fn swap_biomes(sections: &mut [Section], bytes: &[u8]) -> Result<(), String> {
    let mut reader = Reader::new(bytes);
    for section in sections.iter_mut() {
        section.biomes = PalettedContainer::read(Strategy::Biomes, &mut reader, biome_count())
            .map_err(|error| error.to_string())?;
    }
    reader.ensure_eof().map_err(|error| error.to_string())?;
    Ok(())
}
