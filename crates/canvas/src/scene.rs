use std::collections::HashMap;

use block::{BlockGetter, BlockState};
use camera::Camera;
use renderer::RenderScene;
use seed::WorldSeed;
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
    generator: G,
    seed: WorldSeed,
    layout: ChunkLayout,
    chunks: HashMap<(i32, i32), GeneratedChunk>,
}

impl<G: ChunkGenerator> ChunkScene<G> {
    pub fn new(generator: G, seed: WorldSeed, layout: ChunkLayout) -> Self {
        Self {
            generator,
            seed,
            layout,
            chunks: HashMap::new(),
        }
    }

    pub fn loaded(&self) -> usize {
        self.chunks.len()
    }

    /// Generates every chunk within `radius` chunks of the camera that is not
    /// loaded yet.
    pub fn load_around(&mut self, camera: &Camera, radius: u32) -> Result<(), WorldgenError> {
        for coords in chunks_around(camera, radius) {
            if self.chunks.contains_key(&coords) {
                continue;
            }
            let chunk = self
                .generator
                .generate_chunk(self.seed, coords.0, coords.1)?;
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
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use registry::ResourceKey;

    use super::*;

    const LAYOUT: ChunkLayout = ChunkLayout {
        min_y: 0,
        height: 2,
    };

    struct Flat {
        generated: Cell<u32>,
    }

    impl ChunkGenerator for Flat {
        fn generate_chunk(
            &self,
            _seed: WorldSeed,
            x: i32,
            z: i32,
        ) -> Result<GeneratedChunk, WorldgenError> {
            self.generated.set(self.generated.get() + 1);
            let stone = BlockState::new(ResourceKey::new("minecraft", "stone"));
            let air = BlockState::new(ResourceKey::new("minecraft", "air"));
            let mut states = vec![stone; (CHUNK_SIZE * CHUNK_SIZE) as usize];
            states.extend(vec![air; (CHUNK_SIZE * CHUNK_SIZE) as usize]);
            Ok(GeneratedChunk { x, z, states })
        }
    }

    fn scene() -> ChunkScene<Flat> {
        ChunkScene::new(
            Flat {
                generated: Cell::new(0),
            },
            WorldSeed(1),
            LAYOUT,
        )
    }

    #[test]
    fn loads_the_square_around_the_camera_once() {
        let mut scene = scene();
        let camera = Camera::default();
        scene.load_around(&camera, 1).unwrap();
        scene.load_around(&camera, 1).unwrap();
        assert_eq!(scene.loaded(), 9);
        assert_eq!(scene.generator.generated.get(), 9);
    }

    #[test]
    fn reads_blocks_across_chunk_borders() {
        let mut scene = scene();
        scene.load_around(&Camera::default(), 1).unwrap();
        let ground = scene.state_at(-1, 0, -16).unwrap();
        assert_eq!(ground.block, ResourceKey::new("minecraft", "stone"));
        let sky = scene.state_at(15, 1, 15).unwrap();
        assert_eq!(sky.block, ResourceKey::new("minecraft", "air"));
    }

    #[test]
    fn outside_the_loaded_area_has_no_state() {
        let mut scene = scene();
        scene.load_around(&Camera::default(), 0).unwrap();
        assert!(scene.state_at(16, 0, 0).is_none());
        assert!(scene.state_at(0, 2, 0).is_none());
        assert!(scene.state_at(0, -1, 0).is_none());
    }

    #[test]
    fn visible_chunks_are_nearest_first() {
        let mut scene = scene();
        let camera = Camera::default();
        scene.load_around(&camera, 1).unwrap();
        let visible = scene.visible_chunks(&camera);
        assert_eq!(visible.first(), Some(&(0, 0)));
        assert_eq!(visible.len(), 9);
    }

    #[test]
    fn rejects_chunks_with_the_wrong_size() {
        struct Short;
        impl ChunkGenerator for Short {
            fn generate_chunk(
                &self,
                _seed: WorldSeed,
                x: i32,
                z: i32,
            ) -> Result<GeneratedChunk, WorldgenError> {
                Ok(GeneratedChunk {
                    x,
                    z,
                    states: Vec::new(),
                })
            }
        }
        let mut scene = ChunkScene::new(Short, WorldSeed(1), LAYOUT);
        assert!(scene.load_around(&Camera::default(), 0).is_err());
    }
}
