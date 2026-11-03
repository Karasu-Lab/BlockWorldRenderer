use std::collections::BTreeMap;

use registry::ResourceKey;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockState {
    pub block: ResourceKey,
    pub properties: BTreeMap<String, String>,
}

impl BlockState {
    pub fn new(block: ResourceKey) -> Self {
        Self {
            block,
            properties: BTreeMap::new(),
        }
    }
}

pub trait Block {
    fn key(&self) -> &ResourceKey;

    fn default_state(&self) -> BlockState;

    fn is_solid(&self, state: &BlockState) -> bool;
}

pub trait BlockGetter {
    fn state_at(&self, x: i32, y: i32, z: i32) -> Option<BlockState>;
}

pub trait BlockSetter {
    fn set_state(&mut self, x: i32, y: i32, z: i32, state: BlockState);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stone(ResourceKey);

    impl Block for Stone {
        fn key(&self) -> &ResourceKey {
            &self.0
        }

        fn default_state(&self) -> BlockState {
            BlockState::new(self.0.clone())
        }

        fn is_solid(&self, _state: &BlockState) -> bool {
            true
        }
    }

    #[test]
    fn default_state_carries_the_block_key() {
        let stone = Stone(ResourceKey::new("minecraft", "stone"));
        let state = stone.default_state();
        assert_eq!(&state.block, stone.key());
        assert!(stone.is_solid(&state));
    }
}
