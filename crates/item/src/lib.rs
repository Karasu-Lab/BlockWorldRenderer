use bwr_datacomponent::DataComponentPatch;
use bwr_registry::ResourceKey;

#[derive(Debug, Clone, PartialEq)]
pub struct ItemStack {
    pub item: ResourceKey,
    pub count: u32,
    pub components: DataComponentPatch,
}

impl ItemStack {
    pub fn new(item: ResourceKey, count: u32) -> Self {
        Self {
            item,
            count,
            components: DataComponentPatch::default(),
        }
    }
}

pub trait Item {
    fn key(&self) -> &ResourceKey;

    fn max_stack_size(&self) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stack_has_no_component_changes() {
        let stack = ItemStack::new(ResourceKey::new("minecraft", "stick"), 4);
        assert_eq!(stack.count, 4);
        assert!(stack.components.set.is_empty());
        assert!(stack.components.removed.is_empty());
    }
}
