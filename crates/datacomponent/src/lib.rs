use std::collections::{BTreeMap, BTreeSet};

use bwr_registry::ResourceKey;
use bwr_serializer::{SerializerError, Value};

pub trait DataComponent: Into<Value> + TryFrom<Value, Error = SerializerError> {
    fn key() -> ResourceKey;
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataComponentMap {
    entries: BTreeMap<ResourceKey, Value>,
}

impl DataComponentMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_raw(&self, key: &ResourceKey) -> Option<&Value> {
        self.entries.get(key)
    }

    pub fn insert_raw(&mut self, key: ResourceKey, value: Value) -> Option<Value> {
        self.entries.insert(key, value)
    }

    pub fn remove_raw(&mut self, key: &ResourceKey) -> Option<Value> {
        self.entries.remove(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ResourceKey, &Value)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn set<C: DataComponent>(&mut self, component: C) {
        self.entries.insert(C::key(), component.into());
    }

    pub fn component<C: DataComponent>(&self) -> Result<Option<C>, SerializerError> {
        self.entries
            .get(&C::key())
            .cloned()
            .map(C::try_from)
            .transpose()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataComponentPatch {
    pub set: DataComponentMap,
    pub removed: BTreeSet<ResourceKey>,
}

impl DataComponentPatch {
    pub fn apply(&self, base: &DataComponentMap) -> DataComponentMap {
        let mut result = base.clone();
        for key in &self.removed {
            result.remove_raw(key);
        }
        for (key, value) in self.set.iter() {
            result.insert_raw(key.clone(), value.clone());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Damage(i64);

    impl From<Damage> for Value {
        fn from(value: Damage) -> Self {
            Value::Int(value.0)
        }
    }

    impl TryFrom<Value> for Damage {
        type Error = SerializerError;

        fn try_from(value: Value) -> Result<Self, Self::Error> {
            i64::try_from(value).map(Damage)
        }
    }

    impl DataComponent for Damage {
        fn key() -> ResourceKey {
            ResourceKey::new("minecraft", "damage")
        }
    }

    #[test]
    fn stores_and_reads_a_typed_component() {
        let mut map = DataComponentMap::new();
        map.set(Damage(3));
        assert_eq!(map.component::<Damage>(), Ok(Some(Damage(3))));
    }

    #[test]
    fn patch_removes_and_overrides() {
        let mut base = DataComponentMap::new();
        base.set(Damage(3));

        let mut patch = DataComponentPatch::default();
        patch.set.set(Damage(5));
        assert_eq!(
            patch.apply(&base).component::<Damage>(),
            Ok(Some(Damage(5)))
        );

        let mut removal = DataComponentPatch::default();
        removal.removed.insert(Damage::key());
        assert!(removal.apply(&base).is_empty());
    }
}
