use std::collections::HashMap;
use std::sync::Arc;

use block::{BlockGetter, BlockState};
use camera::Camera;
use renderer::RenderScene;
use seed::WorldSeed;
use thread::JoinHandle;
use worldgen::{ChunkGenerator, GeneratedChunk, WorldgenError};

pub const CHUNK_SIZE: i32 = 16;

/// How `GeneratedChunk::states` is indexed: x fastest, then z, then y upwards
/// from `min_y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkLayout {
    pub min_y: i32,
    pub height: u32,
}

impl ChunkLayout {
    pub const OVERWORLD: Self = Self {
        min_y: -64,
        height: 384,
    };

    pub fn len(self) -> usize {
        (CHUNK_SIZE * CHUNK_SIZE) as usize * self.height as usize
    }

    pub fn is_empty(self) -> bool {
        self.height == 0
    }

    fn index(self, local_x: i32, y: i32, local_z: i32) -> Option<usize> {
        let level = y.checked_sub(self.min_y)?;
        if level < 0 || level >= self.height as i32 {
            return None;
        }
        Some(((level * CHUNK_SIZE + local_z) * CHUNK_SIZE + local_x) as usize)
    }
}

/// The chunks around the camera, generated from a fixed seed.
pub struct ChunkScene<G> {
    generator: Arc<G>,
    seed: WorldSeed,
    layout: ChunkLayout,
    chunks: HashMap<(i32, i32), GeneratedChunk>,
    pending: HashMap<(i32, i32), JoinHandle<Result<GeneratedChunk, WorldgenError>>>,
}

impl<G: ChunkGenerator> ChunkScene<G> {
    pub fn new(generator: G, seed: WorldSeed, layout: ChunkLayout) -> Self {
        Self {
            generator: Arc::new(generator),
            seed,
            layout,
            chunks: HashMap::new(),
            pending: HashMap::new(),
        }
    }

    pub fn loaded(&self) -> usize {
        self.chunks.len()
    }

    /// The chunks being generated on workers.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Generates every chunk within `radius` chunks of the camera that is not
    /// loaded yet, on the calling thread.
    pub fn load_around(&mut self, camera: &Camera, radius: u32) -> Result<(), WorldgenError> {
        for coords in chunks_around(camera, radius) {
            if self.chunks.contains_key(&coords) {
                continue;
            }
            let chunk = self
                .generator
                .generate_chunk(self.seed, coords.0, coords.1)?;
            self.accept(coords, chunk)?;
        }
        Ok(())
    }

    fn accept(&mut self, coords: (i32, i32), chunk: GeneratedChunk) -> Result<(), WorldgenError> {
        if chunk.states.len() != self.layout.len() {
            return Err(WorldgenError::Generation(format!(
                "chunk ({}, {}) has {} block states, expected {}",
                coords.0,
                coords.1,
                chunk.states.len(),
                self.layout.len()
            )));
        }
        self.chunks.insert(coords, chunk);
        Ok(())
    }
}

impl<G: ChunkGenerator + Send + Sync + 'static> ChunkScene<G> {
    /// Starts generating, on workers, every chunk within `radius` chunks of the
    /// camera that is neither loaded nor being generated. Never blocks, so the
    /// browser's main thread can call it; [`StreamingScene::poll`] collects the
    /// results.
    pub fn request_around(&mut self, camera: &Camera, radius: u32) -> Result<(), WorldgenError> {
        for coords in chunks_around(camera, radius) {
            if self.chunks.contains_key(&coords) || self.pending.contains_key(&coords) {
                continue;
            }
            let generator = self.generator.clone();
            let seed = self.seed;
            let handle = thread::spawn(format!("chunk-{}-{}", coords.0, coords.1), move || {
                generator.generate_chunk(seed, coords.0, coords.1)
            })
            .map_err(|error| WorldgenError::Generation(error.to_string()))?;
            self.pending.insert(coords, handle);
        }
        Ok(())
    }
}

/// A scene whose chunks arrive while it is drawn.
pub trait StreamingScene: RenderScene {
    /// Takes in the chunks the workers have finished; returns how many.
    fn poll(&mut self) -> Result<usize, WorldgenError>;
}

impl<G: ChunkGenerator + Send + Sync + 'static> StreamingScene for ChunkScene<G> {
    fn poll(&mut self) -> Result<usize, WorldgenError> {
        let ready: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(coords, handle)| handle.try_join().map(|result| (*coords, result)))
            .collect();
        let count = ready.len();
        for (coords, result) in ready {
            self.pending.remove(&coords);
            let chunk = result.map_err(|error| WorldgenError::Generation(error.to_string()))??;
            self.accept(coords, chunk)?;
        }
        Ok(count)
    }
}

impl<G> BlockGetter for ChunkScene<G> {
    fn state_at(&self, x: i32, y: i32, z: i32) -> Option<BlockState> {
        let chunk = self
            .chunks
            .get(&(x.div_euclid(CHUNK_SIZE), z.div_euclid(CHUNK_SIZE)))?;
        let index = self
            .layout
            .index(x.rem_euclid(CHUNK_SIZE), y, z.rem_euclid(CHUNK_SIZE))?;
        chunk.states.get(index).cloned()
    }
}

impl<G> RenderScene for ChunkScene<G> {
    fn visible_chunks(&self, camera: &Camera) -> Vec<(i32, i32)> {
        let (center_x, center_z) = camera_chunk(camera);
        let mut visible: Vec<_> = self.chunks.keys().copied().collect();
        visible.sort_by_key(|&(x, z)| {
            let (dx, dz) = (i64::from(x - center_x), i64::from(z - center_z));
            (dx * dx + dz * dz, x, z)
        });
        visible
    }
}

fn camera_chunk(camera: &Camera) -> (i32, i32) {
    (
        (camera.position[0].floor() as i32).div_euclid(CHUNK_SIZE),
        (camera.position[2].floor() as i32).div_euclid(CHUNK_SIZE),
    )
}

fn chunks_around(camera: &Camera, radius: u32) -> impl Iterator<Item = (i32, i32)> {
    let (center_x, center_z) = camera_chunk(camera);
    let radius = radius as i32;
    (-radius..=radius)
        .flat_map(move |dx| (-radius..=radius).map(move |dz| (center_x + dx, center_z + dz)))
}
