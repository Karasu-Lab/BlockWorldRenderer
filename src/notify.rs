use std::cell::RefCell;
use std::path::PathBuf;

use js_sys::{Array, Object, Reflect, Uint8Array};
use pomme_client::viewer::assets::{Fetch, Hash, Progress, Transfer, Verified};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub type DownloadNotifier;

    #[wasm_bindgen(method, catch)]
    async fn fetch(
        this: &DownloadNotifier,
        url: &str,
        expected: Option<String>,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    async fn store(this: &DownloadNotifier, cache: &str, files: Array) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method)]
    fn progress(
        this: &DownloadNotifier,
        stage: &str,
        done: u32,
        total: u32,
        status: &str,
        file_done: f64,
        file_total: f64,
    );
}

thread_local! {
    static LISTENER: RefCell<Option<DownloadNotifier>> = const { RefCell::new(None) };
}

pub fn listen(notifier: DownloadNotifier) {
    LISTENER.with(|listener| *listener.borrow_mut() = Some(notifier));
}

pub fn notify(progress: &Progress) {
    LISTENER.with(|listener| {
        if let Some(notifier) = listener.borrow().as_ref() {
            notifier.progress(
                progress.stage.as_str(),
                progress.done,
                progress.total,
                &progress.status,
                progress.file_done as f64,
                progress.file_total as f64,
            );
        }
    });
}

fn failure(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            Reflect::get(&error, &JsValue::from_str("message"))
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| format!("{error:?}"))
}

fn descriptor(file: &Fetch) -> Result<Object, String> {
    let object = Object::new();
    let set = |name: &str, value: JsValue| {
        Reflect::set(&object, &JsValue::from_str(name), &value)
            .map(|_| ())
            .map_err(failure)
    };
    set("url", JsValue::from_str(&file.url))?;
    set(
        "key",
        JsValue::from_str(&pomme_vfs::browser::key(&file.path)),
    )?;
    set("size", JsValue::from_f64(file.size as f64))?;
    set("expected", JsValue::from_str(&file.expected))?;
    Ok(object)
}

pub struct NotifyTransfer {
    notifier: DownloadNotifier,
    namespace: &'static str,
}

impl NotifyTransfer {
    pub fn new(notifier: DownloadNotifier, namespace: &'static str) -> Self {
        Self {
            notifier,
            namespace,
        }
    }

    async fn bytes(&self, url: &str, expected: Option<String>) -> Result<Vec<u8>, String> {
        let received = self.notifier.fetch(url, expected).await.map_err(failure)?;
        Ok(Uint8Array::new(&received).to_vec())
    }
}

impl Transfer for NotifyTransfer {
    async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, String> {
        self.bytes(url, None).await
    }

    async fn fetch_verified(
        &self,
        request: &Verified<'_>,
        _on_bytes: &(dyn Fn(u64) + Send + Sync),
    ) -> Result<Vec<u8>, String> {
        if request.hash != Hash::Sha1 {
            return Err("only SHA-1 downloads can be handed to the page".to_owned());
        }
        self.bytes(request.url, Some(request.expected.to_owned()))
            .await
    }

    async fn download_all(
        &self,
        files: Vec<Fetch>,
        _on_progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<(), String> {
        let missing = pomme_install::missing(files);
        if missing.is_empty() {
            return Ok(());
        }
        let descriptors = Array::new();
        for file in &missing {
            descriptors.push(&descriptor(file)?);
        }
        self.notifier
            .store(&pomme_vfs::browser::cache_name(self.namespace), descriptors)
            .await
            .map_err(failure)?;
        let paths: Vec<PathBuf> = missing.iter().map(|file| file.path.clone()).collect();
        pomme_vfs::browser::restore_paths(self.namespace, &paths).await?;
        if pomme_install::missing(missing).is_empty() {
            Ok(())
        } else {
            Err("the page did not store every downloaded file".to_owned())
        }
    }

    fn stores_externally(&self) -> bool {
        true
    }
}
