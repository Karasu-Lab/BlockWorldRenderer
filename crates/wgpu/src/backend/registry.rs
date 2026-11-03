use std::sync::{Arc, PoisonError, RwLock};

/// A generational index. Generation 0 is never issued, so the default key is
/// an empty placeholder that resolves to nothing.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    index: u32,
    generation: u32,
}

impl Key {
    pub fn is_null(self) -> bool {
        self.generation == 0
    }
}

struct Slot<T> {
    generation: u32,
    value: Option<Arc<T>>,
}

pub struct Slab<T> {
    slots: RwLock<SlabInner<T>>,
}

struct SlabInner<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Self {
            slots: RwLock::new(SlabInner {
                slots: Vec::new(),
                free: Vec::new(),
            }),
        }
    }
}

impl<T> Slab<T> {
    pub fn insert(&self, value: T) -> Key {
        let mut inner = self.slots.write().unwrap_or_else(PoisonError::into_inner);
        let value = Some(Arc::new(value));
        if let Some(index) = inner.free.pop() {
            let slot = &mut inner.slots[index as usize];
            slot.value = value;
            Key {
                index,
                generation: slot.generation,
            }
        } else {
            let index = inner.slots.len() as u32;
            inner.slots.push(Slot {
                generation: 1,
                value,
            });
            Key {
                index,
                generation: 1,
            }
        }
    }

    pub fn get(&self, key: Key) -> Option<Arc<T>> {
        let inner = self.slots.read().unwrap_or_else(PoisonError::into_inner);
        let slot = inner.slots.get(key.index as usize)?;
        if slot.generation == key.generation && !key.is_null() {
            slot.value.clone()
        } else {
            None
        }
    }

    /// Replaces the value behind a live key.
    pub fn replace(&self, key: Key, value: T) -> bool {
        let mut inner = self.slots.write().unwrap_or_else(PoisonError::into_inner);
        match inner.slots.get_mut(key.index as usize) {
            Some(slot) if slot.generation == key.generation && slot.value.is_some() => {
                slot.value = Some(Arc::new(value));
                true
            }
            _ => false,
        }
    }

    pub fn remove(&self, key: Key) -> Option<Arc<T>> {
        let mut inner = self.slots.write().unwrap_or_else(PoisonError::into_inner);
        let slot = inner.slots.get_mut(key.index as usize)?;
        if slot.generation != key.generation || key.is_null() {
            return None;
        }
        let value = slot.value.take();
        if value.is_some() {
            slot.generation = slot.generation.wrapping_add(1).max(1);
            inner.free.push(key.index);
        }
        value
    }

    pub fn clear(&self) {
        let mut inner = self.slots.write().unwrap_or_else(PoisonError::into_inner);
        inner.slots.clear();
        inner.free.clear();
    }
}
