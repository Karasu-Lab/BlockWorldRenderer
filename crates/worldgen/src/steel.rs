use std::collections::HashMap;
use std::sync::Mutex;

use block::BlockState;
use pomme_singleplayer::generated::GeneratedWorld;
use registry::ResourceKey;
use seed::WorldSeed;

pub use pomme_singleplayer::generated::{ThreadSpawner, install_thread_spawner};

use crate::{ChunkGenerator, GeneratedChunk, WorldgenError};

/// Overworld terrain from SteelMC, which follows vanilla's generation.
///
/// `GeneratedChunk::states` is indexed x fastest, then z, then y upwards from
/// the dimension's `min_y` (-64 for the overworld, 384 blocks high). The
/// world opens on the first chunk asked for and reopens if the seed changes.
/// Generation blocks, so call it from a worker. On wasm32 install a thread
/// spawner with [`install_thread_spawner`] first.
#[derive(Default)]
pub struct SteelChunkGenerator {
    state: Mutex<Option<Opened>>,
}

struct Opened {
    seed: WorldSeed,
    world: GeneratedWorld,
    /// State ids already turned into block states.
    described: HashMap<u16, BlockState>,
}

impl SteelChunkGenerator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ChunkGenerator for SteelChunkGenerator {
    fn generate_chunk(
        &self,
        seed: WorldSeed,
        x: i32,
        z: i32,
    ) -> Result<GeneratedChunk, WorldgenError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| WorldgenError::Generation("the generator was poisoned".into()))?;
        if guard.as_ref().is_none_or(|opened| opened.seed != seed) {
            let world =
                GeneratedWorld::open_overworld(seed.0, None).map_err(WorldgenError::Generation)?;
            *guard = Some(Opened {
                seed,
                world,
                described: HashMap::new(),
            });
        }
        let opened = guard.as_mut().expect("the world was just opened");
        let ids = opened
            .world
            .block_states(x, z)
            .map_err(WorldgenError::Generation)?;

        let mut states = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(state) = opened.described.get(&id) {
                states.push(state.clone());
                continue;
            }
            let described = opened.world.describe(id).ok_or_else(|| {
                WorldgenError::Generation(format!("block state {id} is not in the registry"))
            })?;
            let mut state = BlockState::new(ResourceKey::new(described.namespace, described.path));
            state.properties.extend(described.properties);
            states.push(opened.described.entry(id).or_insert(state).clone());
        }
        Ok(GeneratedChunk { x, z, states })
    }
}
