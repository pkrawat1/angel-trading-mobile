//! Small JSON documents kept on the device, for state that belongs to this
//! phone rather than to the broker account, such as the watchlist.

use serde::{de::DeserializeOwned, Serialize};

pub fn load<T: DeserializeOwned>(name: &str) -> Option<T> {
    serde_json::from_str(&read(name)?).ok()
}

pub fn save<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
    write(name, &json)
}

#[cfg(not(target_arch = "wasm32"))]
fn read(name: &str) -> Option<String> {
    let path = crate::auth::get_app_data_dir().ok()?.join(format!("{name}.json"));
    std::fs::read_to_string(path).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn write(name: &str, json: &str) -> Result<(), String> {
    let dir = crate::auth::get_app_data_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{name}.json")), json).map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
fn read(name: &str) -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(&format!("angel_trading_{name}"))
        .ok()?
}

#[cfg(target_arch = "wasm32")]
fn write(name: &str, json: &str) -> Result<(), String> {
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("localStorage is unavailable")?;
    storage
        .set_item(&format!("angel_trading_{name}"), json)
        .map_err(|e| format!("{e:?}"))
}
