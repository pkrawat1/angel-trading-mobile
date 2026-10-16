//! Server-side HTTP client for SmartAPI. Headers, retries and the static-IP
//! proxy follow trade_galleon's AngelOne broker, so both apps look the same to
//! Angel One.

use std::sync::OnceLock;
use std::time::Duration;

use dioxus::prelude::ServerFnError;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;

const BASE_URL: &str = "https://apiconnect.angelbroking.com/";
const MAX_ATTEMPTS: u64 = 3;

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

pub async fn get<T: DeserializeOwned>(path: &str, token: &str) -> Result<T, ServerFnError> {
    let url = format!("{BASE_URL}{path}");
    decode(send(|| http().get(&url).headers(headers(Some(token))), true).await?)
}

pub async fn post<T: DeserializeOwned>(
    path: &str,
    token: Option<&str>,
    body: &Value,
) -> Result<T, ServerFnError> {
    let url = format!("{BASE_URL}{path}");
    decode(send(|| http().post(&url).headers(headers(token)).json(body), true).await?)
}

/// For order calls, which must not be retried: a request that timed out may
/// still have reached the exchange, and a retry would place it twice.
pub async fn post_once<T: DeserializeOwned>(
    path: &str,
    token: &str,
    body: &Value,
) -> Result<T, ServerFnError> {
    let url = format!("{BASE_URL}{path}");
    decode(send(|| http().post(&url).headers(headers(Some(token))).json(body), false).await?)
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

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(20));
        // SEBI requires API orders to come from the client's registered static
        // IP. PROXY_URL is that egress, as in the web app's client config.
        if let Some(url) = env("PROXY_URL") {
            match reqwest::Proxy::all(&url) {
                Ok(proxy) => {
                    let proxy = match (env("PROXY_USERNAME"), env("PROXY_PASSWORD")) {
                        (Some(user), Some(pass)) => proxy.basic_auth(&user, &pass),
                        _ => proxy,
                    };
                    builder = builder.proxy(proxy);
                }
                Err(e) => tracing::error!("Ignoring invalid PROXY_URL: {e}"),
            }
        }
        builder.build().unwrap_or_default()
    })
}

fn headers(token: Option<&str>) -> HeaderMap {
    // Angel One matches X-ClientPublicIP against the request's source IP, which
    // is the proxy's when one is configured.
    let proxy_host = env("PROXY_URL").and_then(|url| {
        reqwest::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
    });
    let public_ip = proxy_host
        .or_else(|| env("PUBLIC_IP"))
        .unwrap_or_else(|| "0.0.0.0".into());

    let mut pairs = vec![
        ("Content-Type", "application/json".to_string()),
        ("Accept", "application/json".to_string()),
        ("X-UserType", "USER".to_string()),
        ("X-SourceID", "WEB".to_string()),
        ("X-ClientLocalIP", env("LOCAL_IP").unwrap_or_default()),
        ("X-ClientPublicIP", public_ip),
        ("X-MACAddress", env("MAC_ADDRESS").unwrap_or_default()),
        ("X-PrivateKey", env("API_KEY").unwrap_or_default()),
    ];
    if let Some(token) = token {
        pairs.push(("Authorization", format!("Bearer {token}")));
    }

    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        if let Ok(value) = HeaderValue::from_str(&value) {
            map.insert(name, value);
        }
    }
    map
}

fn fail(message: String) -> ServerFnError {
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
        fail(format!(
            "Angel One returned an unexpected response (HTTP {status})"
        ))
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
