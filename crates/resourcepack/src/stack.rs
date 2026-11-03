use std::collections::{BTreeMap, BTreeSet};

use registry::ResourceKey;

use crate::{ResourcePackError, ResourceProvider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackLayer<T> {
    pub id: String,
    pub priority: i32,
    pub pack: T,
}

/// Packs stacked by priority: a higher pack overrides the ones below it, and
/// among equal priorities the pack inserted last wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackStack<T> {
    layers: Vec<PackLayer<T>>,
}

impl<T> Default for PackStack<T> {
    fn default() -> Self {
        Self { layers: Vec::new() }
    }
}

impl<T> PackStack<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Puts `pack` at `priority`, replacing and returning the pack that had
    /// the same `id`.
    pub fn insert(&mut self, id: impl Into<String>, priority: i32, pack: T) -> Option<T> {
        let id = id.into();
        let replaced = self.remove(&id);
        let at = self.layers.partition_point(|layer| layer.priority <= priority);
        self.layers.insert(at, PackLayer { id, priority, pack });
        replaced
    }

    /// Puts `pack` above every pack already stacked.
    pub fn push(&mut self, id: impl Into<String>, pack: T) -> Option<T> {
        let priority = self.layers.last().map_or(0, |top| top.priority.saturating_add(1));
        self.insert(id, priority, pack)
    }

    pub fn remove(&mut self, id: &str) -> Option<T> {
        let at = self.layers.iter().position(|layer| layer.id == id)?;
        Some(self.layers.remove(at).pack)
    }

    /// Moves the pack to `priority`; false if there is no such pack.
    pub fn set_priority(&mut self, id: &str, priority: i32) -> bool {
        match self.remove(id) {
            Some(pack) => {
                self.insert(id, priority, pack);
                true
            }
            None => false,
        }
    }

    pub fn get(&self, id: &str) -> Option<&PackLayer<T>> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    pub fn priority(&self, id: &str) -> Option<i32> {
        self.get(id).map(|layer| layer.priority)
    }

    /// The packs from the lowest priority to the highest.
    pub fn ascending(&self) -> impl DoubleEndedIterator<Item = &PackLayer<T>> {
        self.layers.iter()
    }

    /// The packs from the highest priority to the lowest.
    pub fn descending(&self) -> impl DoubleEndedIterator<Item = &PackLayer<T>> {
        self.layers.iter().rev()
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&PackLayer<T>) -> bool) {
        self.layers.retain(|layer| keep(layer));
    }
}

impl<T: ResourceProvider> PackStack<T> {
    /// The id of the pack that supplies `key`.
    pub fn owner(&self, key: &ResourceKey) -> Option<&str> {
        self.descending()
            .find(|layer| layer.pack.exists(key))
            .map(|layer| layer.id.as_str())
    }

    /// The ids of every pack holding `key`, from the lowest priority to the
    /// highest.
    pub fn providers(&self, key: &ResourceKey) -> Vec<&str> {
        self.ascending()
            .filter(|layer| layer.pack.exists(key))
            .map(|layer| layer.id.as_str())
            .collect()
    }
}

impl<T: ResourceProvider + ?Sized> ResourceProvider for Box<T> {
    fn namespaces(&self) -> Vec<String> {
        (**self).namespaces()
    }

    fn exists(&self, key: &ResourceKey) -> bool {
        (**self).exists(key)
    }

    fn open(&self, key: &ResourceKey) -> Result<Vec<u8>, ResourcePackError> {
        (**self).open(key)
    }

    fn list(&self, namespace: &str, prefix: &str) -> Vec<ResourceKey> {
        (**self).list(namespace, prefix)
    }
}

impl<T: ResourceProvider> ResourceProvider for PackStack<T> {
    fn namespaces(&self) -> Vec<String> {
        let all: BTreeSet<String> = self
            .layers
            .iter()
            .flat_map(|layer| layer.pack.namespaces())
            .collect();
        all.into_iter().collect()
    }

    fn exists(&self, key: &ResourceKey) -> bool {
        self.layers.iter().any(|layer| layer.pack.exists(key))
    }

    fn open(&self, key: &ResourceKey) -> Result<Vec<u8>, ResourcePackError> {
        self.descending()
            .find(|layer| layer.pack.exists(key))
            .map_or_else(
                || Err(ResourcePackError::NotFound(key.clone())),
                |layer| layer.pack.open(key),
            )
    }

    fn list(&self, namespace: &str, prefix: &str) -> Vec<ResourceKey> {
        let all: BTreeSet<ResourceKey> = self
            .layers
            .iter()
            .flat_map(|layer| layer.pack.list(namespace, prefix))
            .collect();
        all.into_iter().collect()
    }
}

/// A pack held in memory: the built-in resources, or a pack made in code.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryPack {
    files: BTreeMap<ResourceKey, Vec<u8>>,
}

impl MemoryPack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: ResourceKey, bytes: impl Into<Vec<u8>>) {
        self.files.insert(key, bytes.into());
    }
}

impl ResourceProvider for MemoryPack {
    fn namespaces(&self) -> Vec<String> {
        let all: BTreeSet<&str> = self.files.keys().map(ResourceKey::namespace).collect();
        all.into_iter().map(str::to_owned).collect()
    }

    fn exists(&self, key: &ResourceKey) -> bool {
        self.files.contains_key(key)
    }

    fn open(&self, key: &ResourceKey) -> Result<Vec<u8>, ResourcePackError> {
        self.files
            .get(key)
            .cloned()
            .ok_or_else(|| ResourcePackError::NotFound(key.clone()))
    }

    fn list(&self, namespace: &str, prefix: &str) -> Vec<ResourceKey> {
        self.files
            .keys()
            .filter(|key| key.namespace() == namespace && key.path().starts_with(prefix))
            .cloned()
            .collect()
    }
}
