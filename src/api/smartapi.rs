//! Server-side HTTP client for SmartAPI. Headers, retries and the per-client
//! static-IP proxy follow trade_galleon's AngelOne broker, so both apps look
//! the same to Angel One.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use dioxus::prelude::ServerFnError;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;

use super::clients::{self, ClientConfig};
use super::Session;

const ANGEL_ONE_URL: &str = "https://apiconnect.angelbroking.com/";
const MAX_ATTEMPTS: u64 = 3;

/// Tests set this before the first request, so they never depend on the
/// process environment.
#[cfg(test)]
pub static TEST_BASE_URL: OnceLock<String> = OnceLock::new();

/// Where SmartAPI calls go. SMARTAPI_BASE_URL points them at a stand-in such
/// as `cargo run --example mock_smartapi`, so the app can be exercised end to
/// end without a live account.
fn base_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| {
        #[cfg(test)]
        if let Some(url) = TEST_BASE_URL.get() {
            return url.clone();
        }
        let mut url = env("SMARTAPI_BASE_URL").unwrap_or_else(|| ANGEL_ONE_URL.into());
        if !url.ends_with('/') {
            url.push('/');
        }
        url
    })
}

pub(super) fn is_live() -> bool {
    base_url() == ANGEL_ONE_URL
}

// Session errors (expired or invalid JWT) come from Angel's gateway, which spells
// these fields `success` and `errorCode` instead of `status` and `errorcode`.
#[derive(Deserialize)]
struct Envelope {
    #[serde(default, alias = "success")]
    status: bool,
    #[serde(default)]
    message: String,
    #[serde(default, rename = "errorcode", alias = "errorCode")]
    error_code: String,
    #[serde(default)]
    data: Value,
}

pub async fn get<T: DeserializeOwned>(path: &str, session: &Session) -> Result<T, ServerFnError> {
    call(path, &session.client_code, Some(&session.jwt), None, true).await
}

pub async fn post<T: DeserializeOwned>(
    path: &str,
    session: &Session,
    body: &Value,
) -> Result<T, ServerFnError> {
    call(path, &session.client_code, Some(&session.jwt), Some(body), true).await
}

/// For order calls, which must not be retried: a request that timed out may
/// still have reached the exchange, and a retry would place it twice.
pub async fn post_once<T: DeserializeOwned>(
    path: &str,
    session: &Session,
    body: &Value,
) -> Result<T, ServerFnError> {
    call(path, &session.client_code, Some(&session.jwt), Some(body), false).await
}

/// Signs in, before there is a JWT to send.
pub async fn login<T: DeserializeOwned>(client_code: &str, body: &Value) -> Result<T, ServerFnError> {
    let path = "rest/auth/angelbroking/user/v1/loginByPassword";
    call(path, client_code, None, Some(body), true).await
}

async fn call<T: DeserializeOwned>(
    path: &str,
    client_code: &str,
    jwt: Option<&str>,
    body: Option<&Value>,
    retry: bool,
) -> Result<T, ServerFnError> {
    let client = clients::config(client_code).await?;
    let http = http(client.proxy_url.as_deref())?;
    let headers = headers(&client, jwt);
    let url = format!("{}{path}", base_url());
    let request = || {
        let builder = match body {
            Some(body) => http.post(&url).json(body),
            None => http.get(&url),
        };
        builder.headers(headers.clone())
    };
    decode(send(request, retry).await?)
}

/// SmartAPI's candle range format: `YYYY-MM-DD HH:MM` in IST.
pub fn ist_minute(unix_secs: i64) -> String {
    let secs = unix_secs + 5 * 3600 + 30 * 60;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

/// Days since 1970-01-01 to a Gregorian date, after Howard Hinnant's
/// `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub(super) fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// One HTTP client per proxy, so each SmartAPI client's calls leave from its
/// own static IP and still reuse connections.
fn http(proxy: Option<&str>) -> Result<reqwest::Client, ServerFnError> {
    static CLIENTS: OnceLock<Mutex<HashMap<Option<String>, reqwest::Client>>> = OnceLock::new();
    let key = proxy.map(str::to_owned);
    let mut clients = CLIENTS.get_or_init(Default::default).lock().unwrap();
    if let Some(client) = clients.get(&key) {
        return Ok(client.clone());
    }
    let client = build_http(proxy, env("PROXY_USERNAME"), env("PROXY_PASSWORD"))?;
    clients.insert(key, client.clone());
    Ok(client)
}

/// The proxy credentials belong to the Webshare account, so every client's
/// proxy takes the same ones; only the host and port differ.
pub(super) fn build_http(
    proxy: Option<&str>,
    username: Option<String>,
    password: Option<String>,
) -> Result<reqwest::Client, ServerFnError> {
    let builder = reqwest::Client::builder().timeout(Duration::from_secs(20));
    let builder = match proxy {
        // The client's record decides; a system proxy would change the IP
        // Angel One sees.
        None => builder.no_proxy(),
        Some(url) => {
            let proxy = reqwest::Proxy::all(url)
                .map_err(|_| fail("The proxy saved for this client is not a valid URL".into()))?;
            builder.proxy(match (username, password) {
                (Some(user), Some(pass)) => proxy.basic_auth(&user, &pass),
                _ => proxy,
            })
        }
    };
    builder
        .build()
        .map_err(|e| fail(format!("Could not set up the connection to Angel One: {e}")))
}

pub(super) fn headers(client: &ClientConfig, jwt: Option<&str>) -> HeaderMap {
    // Angel One matches X-ClientPublicIP against the request's source IP,
    // which is the proxy's when the client has one.
    let public_ip = client
        .proxy_url
        .as_deref()
        .and_then(proxy_host)
        .or_else(|| env("PUBLIC_IP"))
        .unwrap_or_else(|| "0.0.0.0".into());

    // The local IP and MAC defaults are the web app's.
    let mut pairs = vec![
        ("Content-Type", "application/json".to_string()),
        ("Accept", "application/json".to_string()),
        ("X-UserType", "USER".to_string()),
        ("X-SourceID", "WEB".to_string()),
        ("X-ClientLocalIP", env("LOCAL_IP").unwrap_or_else(|| "192.168.168.168".into())),
        ("X-ClientPublicIP", public_ip),
        ("X-MACAddress", env("MAC_ADDRESS").unwrap_or_else(|| "fe80::216e:6507:4b90:3719".into())),
        ("X-PrivateKey", client.api_key.clone()),
    ];
    if let Some(jwt) = jwt {
        pairs.push(("Authorization", format!("Bearer {jwt}")));
    }

    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        if let Ok(value) = HeaderValue::from_str(&value) {
            map.insert(name, value);
        }
    }
    map
}

/// Proxies are saved as URLs, with or without the scheme.
fn proxy_host(url: &str) -> Option<String> {
    let url = if url.contains("://") { url.to_owned() } else { format!("http://{url}") };
    reqwest::Url::parse(&url).ok()?.host_str().map(str::to_owned)
}

pub(super) fn fail(message: String) -> ServerFnError {
    ServerFnError::ServerError(message)
}

async fn send(
    request: impl Fn() -> reqwest::RequestBuilder,
    retry: bool,
) -> Result<Value, ServerFnError> {
    let mut attempt = 1;
    let response = loop {
        let result = request().send().await;
        // SmartAPI answers a rate-limit breach with 403.
        let transient = match &result {
            Ok(response) => matches!(response.status().as_u16(), 403 | 429 | 500..=599),
            Err(_) => true,
        };
        if retry && transient && attempt < MAX_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(500 * attempt)).await;
            attempt += 1;
            continue;
        }
        break result.map_err(|e| fail(format!("Could not reach Angel One: {e}")))?;
    };

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| fail(format!("Could not read Angel One's response: {e}")))?;
    let envelope: Envelope = serde_json::from_str(&text).map_err(|_| {
        // A rate-limit refusal is a line of plain text, which says more than
        // the status alone. HTML error pages are not worth showing.
        let text = text.trim();
        if !text.is_empty() && text.len() <= 200 && !text.starts_with('<') {
            fail(format!("{text} (HTTP {status})"))
        } else {
            fail(format!("Angel One returned an unexpected response (HTTP {status})"))
        }
    })?;

    if envelope.status {
        Ok(envelope.data)
    } else if envelope.error_code.is_empty() {
        Err(fail(envelope.message))
    } else {
        Err(fail(format!("{} [{}]", envelope.message, envelope.error_code)))
    }
}

fn decode<T: DeserializeOwned>(data: Value) -> Result<T, ServerFnError> {
    let parsed = if data.is_null() {
        // Empty results come back as `null`: lists decode as empty, objects
        // as their defaults.
        serde_json::from_value(Value::Array(Vec::new()))
            .or_else(|_| serde_json::from_value(Value::Object(Default::default())))
    } else {
        serde_json::from_value(data)
    };
    parsed.map_err(|e| fail(format!("Could not read Angel One's response: {e}")))
}
