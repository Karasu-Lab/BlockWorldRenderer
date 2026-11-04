use std::cell::RefCell;
use std::path::PathBuf;

use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::{CacheError, Entry, cache_name, key, path_of};

#[wasm_bindgen]
extern "C" {
    pub type CacheNotifier;

    #[wasm_bindgen(method, catch, js_name = loadCache)]
    async fn load_cache(
        this: &CacheNotifier,
        cache: &str,
        keys: JsValue,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = saveCache)]
    async fn save_cache(
        this: &CacheNotifier,
        cache: &str,
        entries: Array,
    ) -> Result<JsValue, JsValue>;
}

thread_local! {
    static NOTIFIER: RefCell<Option<CacheNotifier>> = const { RefCell::new(None) };
}

pub fn listen(notifier: CacheNotifier) {
    NOTIFIER.with(|slot| *slot.borrow_mut() = Some(notifier));
}

fn notifier() -> Option<CacheNotifier> {
    NOTIFIER.with(|slot| slot.borrow().clone())
}

fn message(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            Reflect::get(&error, &JsValue::from_str("message"))
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| format!("{error:?}"))
}

fn entries(answer: JsValue) -> Result<Vec<Entry>, CacheError> {
    let list: Array = answer
        .dyn_into()
        .map_err(|value| CacheError::Malformed(format!("{value:?}")))?;
    let mut entries = Vec::with_capacity(list.length() as usize);
    for item in list.iter() {
        let pair: Array = item
            .dyn_into()
            .map_err(|value| CacheError::Malformed(format!("{value:?}")))?;
        let Some(path) = pair.get(0).as_string().and_then(|key| path_of(&key)) else {
            continue;
        };
        let bytes: Uint8Array = pair
            .get(1)
            .dyn_into()
            .map_err(|value| CacheError::Malformed(format!("{value:?}")))?;
        entries.push(Entry {
            path,
            bytes: bytes.to_vec(),
        });
    }
    Ok(entries)
}

async fn request(namespace: &str, keys: JsValue) -> Result<Vec<Entry>, CacheError> {
    let Some(notifier) = notifier() else {
        return Ok(Vec::new());
    };
    let answer = notifier
        .load_cache(&cache_name(namespace), keys)
        .await
        .map_err(|error| CacheError::Page(message(error)))?;
    entries(answer)
}

pub async fn load(namespace: &str) -> Result<Vec<Entry>, CacheError> {
    request(namespace, JsValue::UNDEFINED).await
}

pub async fn load_paths(namespace: &str, paths: &[PathBuf]) -> Result<Vec<Entry>, CacheError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Array = paths
        .iter()
        .map(|path| JsValue::from_str(&key(path)))
        .collect();
    request(namespace, keys.into()).await
}

pub async fn save(namespace: &str, entries: &[Entry]) -> Result<(), CacheError> {
    let Some(notifier) = notifier() else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }
    let list = Array::new();
    for entry in entries {
        let pair = Array::new();
        pair.push(&JsValue::from_str(&key(&entry.path)));
        pair.push(&Uint8Array::from(entry.bytes.as_slice()).into());
        list.push(&pair);
    }
    notifier
        .save_cache(&cache_name(namespace), list)
        .await
        .map_err(|error| CacheError::Page(message(error)))?;
    Ok(())
}
