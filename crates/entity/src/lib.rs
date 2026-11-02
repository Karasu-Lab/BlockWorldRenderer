use bwr_datacomponent::DataComponentMap;
use bwr_registry::ResourceKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub u64);

pub trait Entity {
    fn id(&self) -> EntityId;

    fn entity_type(&self) -> &ResourceKey;

    fn position(&self) -> [f64; 3];

    fn components(&self) -> &DataComponentMap;

    fn components_mut(&mut self) -> &mut DataComponentMap;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Marker {
        id: EntityId,
        kind: ResourceKey,
        components: DataComponentMap,
    }

    impl Entity for Marker {
        fn id(&self) -> EntityId {
            self.id
        }

        fn entity_type(&self) -> &ResourceKey {
            &self.kind
        }

        fn position(&self) -> [f64; 3] {
            [0.0; 3]
        }

        fn components(&self) -> &DataComponentMap {
            &self.components
        }

        fn components_mut(&mut self) -> &mut DataComponentMap {
            &mut self.components
        }
    }

    #[test]
    fn entity_exposes_its_identity() {
        let entity = Marker {
            id: EntityId(7),
            kind: ResourceKey::new("minecraft", "marker"),
            components: DataComponentMap::new(),
        };
        assert_eq!(entity.id(), EntityId(7));
        assert!(entity.components().is_empty());
    }
}
