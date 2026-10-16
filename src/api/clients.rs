//! Each client's SmartAPI key and static-IP proxy. The web app keeps both in
//! Firebase under `clients/<client code>`, encrypted with Plug.Crypto, so a
//! client set up there works here without registering twice.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use dioxus::prelude::ServerFnError;
use serde_json::Value;

use super::etf::Term;
use super::plug_crypto;
use super::smartapi::{env, fail, is_live};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientConfig {
    pub api_key: String,
    /// The client's own proxy, such as `http://203.0.113.7:6001/`. Angel One
    /// accepts a key's calls only from the static IP registered with it.
    pub proxy_url: Option<String>,
}

/// The `Utils.encrypt/2` context the web app stores client records under.
const CONTEXT: &str = "client_tokens";
// The web app caches records for as long, so both pick up a key or proxy
// changed in Firebase at about the same time.
const TTL: Duration = Duration::from_secs(5 * 60 * 60);

pub async fn config(client_code: &str) -> Result<Arc<ClientConfig>, ServerFnError> {
    if !is_live() {
        // A stand-in such as the mock needs no key, and its fake clients have
        // no records.
        return Ok(Arc::default());
    }
    let (Some(firebase), Some(auth), Some(key)) =
        (env("FIREBASE_API"), env("FIREBASE_TOKEN"), env("ENCRYPTION_KEY"))
    else {
        // Without the web app's store, one account's key and proxy come from
        // the environment.
        return Ok(Arc::new(ClientConfig {
            api_key: env("API_KEY").unwrap_or_default(),
            proxy_url: env("PROXY_URL"),
        }));
    };

    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Arc<ClientConfig>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, config)) = cache.lock().unwrap().get(client_code) {
        if at.elapsed() < TTL {
            return Ok(config.clone());
        }
    }
    let config = Arc::new(fetch(&firebase, &auth, &key, client_code).await?);
    cache
        .lock()
        .unwrap()
        .insert(client_code.to_string(), (Instant::now(), config.clone()));
    Ok(config)
}

/// Reads one client's record from the Firebase REST API at `base`.
pub async fn fetch(
    base: &str,
    auth: &str,
    encryption_key: &str,
    client_code: &str,
) -> Result<ClientConfig, ServerFnError> {
    let mut url = reqwest::Url::parse(base)
        .ok()
        .filter(|url| !url.cannot_be_a_base())
        .ok_or_else(|| fail("FIREBASE_API is not a URL".into()))?;
    url.path_segments_mut()
        .expect("checked above")
        .pop_if_empty()
        .push("clients")
        .push(&format!("{client_code}.json"));
    url.query_pairs_mut().append_pair("auth", auth);

    // The URL carries the Firebase secret, so errors leave it out.
    let unreachable = |e: reqwest::Error| {
        fail(format!("Could not load the settings for {client_code}: {}", e.without_url()))
    };
    let response = http().get(url).send().await.map_err(unreachable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(fail(format!(
            "Could not load the settings for {client_code} (Firebase answered HTTP {status})"
        )));
    }
    let token = match response.json::<Value>().await.map_err(unreachable)? {
        Value::String(token) => token,
        Value::Null => {
            return Err(fail(format!(
                "Client {client_code} is not set up. Add it in the web app first, with its API key and proxy."
            )))
        }
        _ => return Err(fail(format!("The settings saved for {client_code} are not readable"))),
    };
    let record = plug_crypto::decrypt(encryption_key, CONTEXT, &token).ok_or_else(|| {
        fail(format!(
            "Could not decrypt the settings for {client_code}. ENCRYPTION_KEY must match the web app's."
        ))
    })?;
    from_record(&record).ok_or_else(|| {
        fail(format!("Client {client_code} has no API key saved. Add one in the web app."))
    })
}

/// The record also holds the client's PIN, TOTP secret and the web app's own
/// tokens. This app signs in with what the user types, so it reads only these.
fn from_record(record: &Term) -> Option<ClientConfig> {
    let api_key = record.get("api_key")?.as_str()?.trim();
    if api_key.is_empty() {
        return None;
    }
    let proxy_url = record
        .get("proxy_url")
        .and_then(Term::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned);
    Some(ClientConfig { api_key: api_key.to_owned(), proxy_url })
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}
