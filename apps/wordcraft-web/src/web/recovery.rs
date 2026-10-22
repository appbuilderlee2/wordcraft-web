//! Durable browser recovery snapshots for WordCraft documents.
//! A separate IndexedDB database avoids changing the existing fonts schema.
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen::closure::Closure;
use web_sys::{IdbDatabase, IdbRequest, IdbTransactionMode};

const DB: &str = "wordcraft-recovery-v1";
const STORE: &str = "snapshots";
const KEY: &str = "current";
const MAX_BYTES: usize = 32 * 1024 * 1024;

fn err(e: JsValue) -> String { format!("{e:?}") }

async fn request(req: &IdbRequest) -> Result<JsValue, String> {
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let ok_req = req.clone();
        let ok = Closure::once_into_js(move || {
            let _ = resolve.call1(&JsValue::NULL, &ok_req.result().unwrap_or(JsValue::UNDEFINED));
        });
        let failure = Closure::once_into_js(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB request failed"));
        });
        req.set_onsuccess(Some(ok.unchecked_ref()));
        req.set_onerror(Some(failure.unchecked_ref()));
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.map_err(err)
}

async fn db() -> Result<IdbDatabase, String> {
    let factory = web_sys::window().ok_or("Missing window")?.indexed_db().map_err(err)?.ok_or("No IndexedDB")?;
    let open = factory.open_with_u32(DB, 1).map_err(err)?;
    let upgrade_req = open.clone();
    let upgrade = Closure::<dyn FnMut()>::new(move || {
        if let Ok(db) = upgrade_req.result().and_then(|x| x.dyn_into::<IdbDatabase>().map_err(JsValue::from)) {
            if !db.object_store_names().contains(STORE) { let _ = db.create_object_store(STORE); }
        }
    });
    open.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let result = request(&open).await;
    open.set_onupgradeneeded(None);
    drop(upgrade);
    result?.dyn_into().map_err(err)
}

async fn write(bytes: Vec<u8>) -> Result<(), String> {
    if bytes.len() > MAX_BYTES { return Err("Document exceeds 32 MiB recovery limit".into()); }
    let database = db().await?;
    let store = database.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite).map_err(err)?.object_store(STORE).map_err(err)?;
    let req = store.put_with_key(&js_sys::Uint8Array::from(bytes.as_slice()), &JsValue::from_str(KEY)).map_err(err)?;
    request(&req).await.map(|_| ())
}

async fn read() -> Result<Option<Vec<u8>>, String> {
    let database = db().await?;
    let store = database.transaction_with_str(STORE).map_err(err)?.object_store(STORE).map_err(err)?;
    let value = request(&store.get(&JsValue::from_str(KEY)).map_err(err)?).await?;
    if value.is_undefined() || value.is_null() { return Ok(None); }
    let array: js_sys::Uint8Array = value.dyn_into().map_err(err)?;
    if array.length() as usize > MAX_BYTES { return Err("Recovery snapshot too large".into()); }
    Ok(Some(array.to_vec()))
}

async fn delete() -> Result<(), String> {
    let database = db().await?;
    let store = database.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite).map_err(err)?.object_store(STORE).map_err(err)?;
    let req = store.delete(&JsValue::from_str(KEY)).map_err(err)?;
    request(&req).await.map(|_| ())
}

/// Discard a stale recovery copy once the document has been saved successfully.
pub fn clear() {
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = delete().await { log::warn!("Could not clear recovery snapshot: {e}"); }
    });
}

pub fn save(doc: &wordcraft_doc::Document) {
    let bytes = match wordcraft_engine::io::save_bytes("recovery.json", doc) {
        Ok(bytes) => bytes,
        Err(e) => { log::error!("Recovery serialization failed: {e}"); return; }
    };
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = write(bytes).await { log::error!("Recovery snapshot could not be saved: {e}"); }
    });
}

pub fn offer_restore(inbox: wordcraft_ui_egui::Inbox, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        match read().await {
            Ok(Some(bytes)) => {
                if wordcraft_engine::io::open_bytes("recovery.json", &bytes).is_err() {
                    log::warn!("Stored recovery snapshot is invalid; keeping it for inspection");
                    return;
                }
                let restore = web_sys::window().and_then(|w| w.confirm_with_message("WordCraft 發現上次自動儲存的文件。要恢復嗎？").ok()).unwrap_or(false);
                if restore {
                    inbox.lock().unwrap_or_else(|e| e.into_inner()).push(("recovery.json".into(), bytes));
                    ctx.request_repaint();
                }
            }
            Ok(None) => {}
            Err(e) => log::warn!("Could not read recovery snapshot: {e}"),
        }
    });
}
