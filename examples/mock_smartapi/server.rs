//! A stand-in for Angel One's SmartAPI, shared by the server-function tests
//! and by `cargo run --example mock_smartapi` for trying the app without a
//! live account.
//!
//! Responses follow SmartAPI's documented shapes, quirks included: numbers
//! sent as strings, `null` for empty lists, and gateway errors that spell
//! their fields differently from broker errors. Requests are checked the way
//! Angel One checks them where a mistake would cost a real order or a failed
//! screen: order fields must be strings, stop-loss orders need the STOPLOSS
//! variety, quotes take at most 50 tokens, and candle ranges have per-interval
//! limits.
//!
//! The session token selects a scenario, so tests running in parallel can
//! share one server. Tokens starting with `expired`, `broker-down`, `html` or
//! `empty` always get that failure; `rate-limited` and `flaky` fail only the
//! first call to each endpoint. Any other token gets the fixtures. A login
//! returns `<client code>-session`, so signing in as EXPIRED, BROKER-DOWN and
//! so on selects a scenario from the app.
//!
//! It also stands in for the web app's Firebase store at
//! `/clients/<client code>.json`. Its records were encrypted by the web app's
//! own plug_crypto under ENCRYPTION_KEY, and it refuses requests that lack
//! FIREBASE_TOKEN, as Firebase does.

// The example binary and the unit tests each use a different part of this file.
#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::{Json, Router};
use serde_json::{json, Value};

pub const PROFILE: &str = include_str!("fixtures/profile.json");
pub const RMS: &str = include_str!("fixtures/rms.json");
pub const HOLDINGS: &str = include_str!("fixtures/holdings.json");
pub const ORDER_BOOK: &str = include_str!("fixtures/order_book.json");
pub const TRADE_BOOK: &str = include_str!("fixtures/trade_book.json");
pub const POSITIONS: &str = include_str!("fixtures/positions.json");
pub const QUOTES: &str = include_str!("fixtures/quotes.json");
pub const INSTRUMENTS: &str = include_str!("fixtures/instruments.json");
pub const FIREBASE_CLIENTS: &str = include_str!("fixtures/firebase_clients.json");
/// A throwaway key for the fixture records, not the web app's.
pub const ENCRYPTION_KEY: &str = "mobile-app-test-encryption-key";
pub const FIREBASE_TOKEN: &str = "mock-firebase-token";

const MAX_QUOTE_TOKENS: usize = 50;

pub fn fixture(raw: &str) -> Value {
    serde_json::from_str(raw).expect("fixtures are valid JSON")
}

#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: Value,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    pub fn token(&self) -> Option<&str> {
        self.header("authorization")?.strip_prefix("Bearer ")
    }
}

#[derive(Default)]
pub struct Mock {
    requests: Mutex<Vec<Recorded>>,
    books: Mutex<HashMap<String, Vec<Value>>>,
    attempts: Mutex<HashMap<(String, String), u32>>,
    next_order: AtomicU64,
}

impl Mock {
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    pub fn requests_for(&self, token: &str) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.token() == Some(token))
            .collect()
    }

    fn attempt(&self, token: &str, endpoint: &str) -> u32 {
        let mut attempts = self.attempts.lock().unwrap();
        let count = attempts
            .entry((token.to_string(), endpoint.to_string()))
            .or_default();
        *count += 1;
        *count
    }

    fn with_book<T>(&self, token: &str, f: impl FnOnce(&mut Vec<Value>) -> T) -> T {
        let mut books = self.books.lock().unwrap();
        f(books.entry(token.to_string()).or_insert_with(|| {
            fixture(ORDER_BOOK).as_array().cloned().unwrap_or_default()
        }))
    }
}

pub fn router(mock: Arc<Mock>) -> Router {
    Router::new().fallback(handle).with_state(mock)
}

/// Serves on the current Tokio runtime. Port 0 picks a free port; the bound
/// address is returned.
pub async fn serve(addr: SocketAddr) -> std::io::Result<(SocketAddr, Arc<Mock>)> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    let mock = Arc::new(Mock::default());
    let app = router(mock.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((bound, mock))
}

async fn handle(
    State(mock): State<Arc<Mock>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let recorded = Recorded {
        method: method.clone(),
        path: uri.path().to_string(),
        query: uri.query().map(str::to_owned),
        headers,
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    };
    mock.requests.lock().unwrap().push(recorded.clone());

    if let Some(client_code) = uri
        .path()
        .strip_prefix("/clients/")
        .and_then(|rest| rest.strip_suffix(".json"))
    {
        return firebase_client(uri.query(), client_code);
    }

    // Angel One documents some paths with a trailing slash and accepts both.
    let path = uri.path().trim_end_matches('/');
    let Some((endpoint, expected)) = route(path) else {
        let body = error_body(&format!("No SmartAPI endpoint at {path}"), "MOCK404");
        return (StatusCode::NOT_FOUND, body).into_response();
    };
    if method != expected {
        let body = error_body(&format!("{endpoint} expects {expected}"), "MOCK405");
        return (StatusCode::METHOD_NOT_ALLOWED, body).into_response();
    }
    let token = recorded.token().unwrap_or_default();
    if path.starts_with("/rest/secure/") {
        if token.is_empty() {
            return gateway_error("Token missing", "AG8003");
        }
        if let Some(response) = scenario(&mock, token, endpoint) {
            return response;
        }
    }

    let body = &recorded.body;
    match endpoint {
        "loginByPassword" => login(body),
        "logout" => ok(json!("")),
        "getProfile" => ok(fixture(PROFILE)),
        "getRMS" => ok(fixture(RMS)),
        "getHolding" => ok(fixture(HOLDINGS)),
        "getOrderBook" => ok(Value::Array(mock.with_book(token, |book| book.clone()))),
        "getTradeBook" => ok(fixture(TRADE_BOOK)),
        "getPosition" => ok(fixture(POSITIONS)),
        "quote" => quote(body),
        "getCandleData" => candles(body),
        "searchScrip" => search(body),
        "placeOrder" => place_order(&mock, token, body),
        "modifyOrder" => modify_order(&mock, token, body),
        "cancelOrder" => cancel_order(&mock, token, body),
        _ => estimate_charges(body),
    }
}

/// The endpoint name and method for each SmartAPI path the app calls. A path
/// with a typo gets a 404 here, as it would from Angel One.
fn route(path: &str) -> Option<(&'static str, Method)> {
    Some(match path.strip_prefix("/rest/")? {
        "auth/angelbroking/user/v1/loginByPassword" => ("loginByPassword", Method::POST),
        "secure/angelbroking/user/v1/logout" => ("logout", Method::POST),
        "secure/angelbroking/user/v1/getProfile" => ("getProfile", Method::GET),
        "secure/angelbroking/user/v1/getRMS" => ("getRMS", Method::GET),
        "secure/angelbroking/portfolio/v1/getHolding" => ("getHolding", Method::GET),
        "secure/angelbroking/order/v1/getOrderBook" => ("getOrderBook", Method::GET),
        "secure/angelbroking/order/v1/getTradeBook" => ("getTradeBook", Method::GET),
        "secure/angelbroking/order/v1/getPosition" => ("getPosition", Method::GET),
        "secure/angelbroking/market/v1/quote" => ("quote", Method::POST),
        "secure/angelbroking/historical/v1/getCandleData" => ("getCandleData", Method::POST),
        "secure/angelbroking/order/v1/searchScrip" => ("searchScrip", Method::POST),
        "secure/angelbroking/order/v1/placeOrder" => ("placeOrder", Method::POST),
        "secure/angelbroking/order/v1/modifyOrder" => ("modifyOrder", Method::POST),
        "secure/angelbroking/order/v1/cancelOrder" => ("cancelOrder", Method::POST),
        "secure/angelbroking/brokerage/v1/estimateCharges" => ("estimateCharges", Method::POST),
        _ => return None,
    })
}

fn scenario(mock: &Mock, token: &str, endpoint: &str) -> Option<Response> {
    let first_call = || mock.attempt(token, endpoint) == 1;
    Some(if token.starts_with("expired") {
        gateway_error("Invalid Token", "AG8001")
    } else if token.starts_with("broker-down") {
        broker_error("Something Went Wrong, Please Try After Sometime", "AB1004")
    } else if token.starts_with("html") {
        let page = Html("<html><body><h1>502 Bad Gateway</h1></body></html>");
        (StatusCode::BAD_GATEWAY, page).into_response()
    } else if token.starts_with("empty") {
        ok(Value::Null)
    } else if token.starts_with("rate-limited") && first_call() {
        let text = "Access denied because of exceeding access rate";
        (StatusCode::FORBIDDEN, text).into_response()
    } else if token.starts_with("flaky") && first_call() {
        let body = error_body("Internal Error, Please try after sometime", "AB2001");
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    } else {
        return None;
    })
}

/// Firebase answers an unknown key with `null`, not an error.
fn firebase_client(query: Option<&str>, client_code: &str) -> Response {
    if query != Some(format!("auth={FIREBASE_TOKEN}").as_str()) {
        let body = Json(json!({ "error": "Permission denied" }));
        return (StatusCode::UNAUTHORIZED, body).into_response();
    }
    let record = fixture(FIREBASE_CLIENTS).get(client_code).cloned();
    Json(record.unwrap_or(Value::Null)).into_response()
}

fn ok(data: Value) -> Response {
    Json(json!({ "status": true, "message": "SUCCESS", "errorcode": "", "data": data }))
        .into_response()
}

fn error_body(message: &str, code: &str) -> Json<Value> {
    Json(json!({ "status": false, "message": message, "errorcode": code, "data": null }))
}

/// The broker reports a rejected request with HTTP 200 and `status: false`.
fn broker_error(message: &str, code: &str) -> Response {
    error_body(message, code).into_response()
}

fn gateway_error(message: &str, code: &str) -> Response {
    let body = json!({ "success": false, "message": message, "errorCode": code, "data": "" });
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}

fn login(body: &Value) -> Response {
    let field = |key: &str| body[key].as_str().unwrap_or_default();
    let (client_code, password, totp) = (field("clientcode"), field("password"), field("totp"));
    if client_code.is_empty() || password.is_empty() {
        return broker_error("Invalid Email Or Password", "AB1000");
    }
    if totp.len() != 6 || !totp.bytes().all(|b| b.is_ascii_digit()) || totp == "000000" {
        return broker_error("Invalid totp", "AB1050");
    }
    ok(json!({
        "jwtToken": format!("{}-session", client_code.to_ascii_lowercase()),
        "refreshToken": "mock-refresh-token",
        "feedToken": "mock-feed-token",
        "state": ""
    }))
}

fn instrument(exchange: &str, token: &str) -> Option<Value> {
    fixture(INSTRUMENTS)
        .as_array()?
        .iter()
        .find(|i| i["exchange"] == exchange && i["symboltoken"] == token)
        .cloned()
}

fn quote(body: &Value) -> Response {
    if !matches!(body["mode"].as_str(), Some("FULL" | "OHLC" | "LTP")) {
        return broker_error("Invalid mode", "AB4006");
    }
    let Some(requested) = body["exchangeTokens"].as_object() else {
        return broker_error("Invalid exchangeTokens", "AB4019");
    };
    let total: usize = requested
        .values()
        .map(|tokens| tokens.as_array().map_or(0, Vec::len))
        .sum();
    if total > MAX_QUOTE_TOKENS {
        return broker_error("Quote API allows at most 50 tokens per request", "AB4020");
    }

    let known = fixture(QUOTES);
    let (mut fetched, mut unfetched) = (Vec::new(), Vec::new());
    for (exchange, tokens) in requested {
        for token in tokens.as_array().into_iter().flatten().filter_map(Value::as_str) {
            let full = known
                .as_array()
                .into_iter()
                .flatten()
                .find(|q| q["exchange"] == exchange.as_str() && q["symbolToken"] == token)
                .cloned()
                .or_else(|| instrument(exchange, token).map(|seed| synthesize_quote(&seed)));
            match full {
                Some(q) => fetched.push(q),
                None => unfetched.push(json!({
                    "exchange": exchange, "symbolToken": token, "message": "", "errorCode": ""
                })),
            }
        }
    }
    ok(json!({ "fetched": fetched, "unfetched": unfetched }))
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// A FULL-mode quote built from an instrument's seed prices, for instruments
/// without a hand-written one in quotes.json.
fn synthesize_quote(seed: &Value) -> Value {
    let ltp = seed["ltp"].as_f64().unwrap_or(100.0);
    let close = seed["close"].as_f64().unwrap_or(ltp);
    let token = seed["symboltoken"].as_str().unwrap_or_default();
    let index = token.starts_with("999");
    let open = round2(close + (ltp - close) * 0.35);
    let high = round2(ltp.max(open) * 1.004);
    let low = round2(ltp.min(open) * 0.996);
    let tick = (ltp * 0.0004).max(0.05);
    let level = |side: f64, i: usize| {
        json!({
            "price": round2(ltp + side * tick * (i as f64 + 1.0)),
            "quantity": 40 * (i + 1) + token.len() * 7,
            "orders": i + 1
        })
    };
    let (buy, sell): (Vec<Value>, Vec<Value>) = if index {
        (Vec::new(), Vec::new())
    } else {
        ((0..5).map(|i| level(-1.0, i)).collect(), (0..5).map(|i| level(1.0, i)).collect())
    };
    let volume: i64 = if index { 0 } else { 250_000 + token.len() as i64 * 91_337 };
    let last_qty: i64 = if index { 0 } else { 15 };
    let avg = if index { 0.0 } else { round2((high + low + ltp) / 3.0) };
    let (lower, upper) = if index { (0.0, 0.0) } else { (round2(close * 0.9), round2(close * 1.1)) };
    json!({
        "exchange": seed["exchange"],
        "tradingSymbol": seed["tradingsymbol"],
        "symbolToken": token,
        "ltp": ltp,
        "open": open,
        "high": high,
        "low": low,
        "close": close,
        "lastTradeQty": last_qty,
        "exchFeedTime": "16-Oct-2026 15:29:59",
        "exchTradeTime": "16-Oct-2026 15:29:58",
        "netChange": round2(ltp - close),
        "percentChange": round2((ltp - close) / close * 100.0),
        "avgPrice": avg,
        "tradeVolume": volume,
        "opnInterest": 0,
        "lowerCircuit": lower,
        "upperCircuit": upper,
        "totBuyQuan": volume / 20,
        "totSellQuan": volume / 17,
        "52WeekLow": round2(close * 0.74),
        "52WeekHigh": round2(close * 1.16),
        "depth": { "buy": buy, "sell": sell }
    })
}

/// SmartAPI's bar size and longest range, in minutes and days. Daily bars
/// have no intraday size.
fn interval_limits(interval: &str) -> Option<(i64, i64)> {
    Some(match interval {
        "ONE_MINUTE" => (1, 30),
        "THREE_MINUTE" => (3, 60),
        "FIVE_MINUTE" => (5, 100),
        "TEN_MINUTE" => (10, 100),
        "FIFTEEN_MINUTE" => (15, 200),
        "THIRTY_MINUTE" => (30, 200),
        "ONE_HOUR" => (60, 400),
        "ONE_DAY" => (0, 2000),
        _ => return None,
    })
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

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

/// Minutes since 1970-01-01 00:00 IST for SmartAPI's `YYYY-MM-DD HH:MM`, the
/// only format `getCandleData` accepts.
pub fn parse_ist_minute(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 16 || b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' {
        return None;
    }
    let num = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let (y, mo, d, h, mi) = (num(0, 4)?, num(5, 7)?, num(8, 10)?, num(11, 13)?, num(14, 16)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 1440 + h * 60 + mi)
}

fn candle_time(minute: i64) -> String {
    let (y, m, d) = civil_from_days(minute.div_euclid(1440));
    let rem = minute.rem_euclid(1440);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:00+05:30", rem / 60, rem % 60)
}

/// A seeded random walk, so a symbol's chart looks the same on every request.
struct Walk(u64);

impl Walk {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn candles(body: &Value) -> Response {
    let interval = body["interval"].as_str().unwrap_or_default();
    let Some((step, max_days)) = interval_limits(interval) else {
        return broker_error("Invalid interval", "AB13000");
    };
    let exchange = body["exchange"].as_str().unwrap_or_default();
    let token = body["symboltoken"].as_str().unwrap_or_default();
    if exchange.is_empty() || token.is_empty() {
        return broker_error("Invalid exchange or symboltoken", "AB13000");
    }
    let range = (
        body["fromdate"].as_str().and_then(parse_ist_minute),
        body["todate"].as_str().and_then(parse_ist_minute),
    );
    let (Some(from), Some(to)) = range else {
        return broker_error("Invalid date or time format", "AB13000");
    };
    if to < from || to - from > max_days * 1440 {
        return broker_error("Date range exceeds the limit for this interval", "AB13000");
    }

    let seed = instrument(exchange, token);
    let mut last = seed.as_ref().and_then(|s| s["close"].as_f64()).unwrap_or(100.0);
    let mut walk = Walk(token.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    }));
    let swing = 0.004 * (if step == 0 { 375.0 } else { step as f64 / 5.0 }).sqrt().min(5.0);
    let mut bar = |minute: i64| {
        let open = last;
        let close = round2(open * (1.0 + (walk.unit() - 0.5) * swing));
        let high = round2(open.max(close) * (1.0 + walk.unit() * swing / 3.0));
        let low = round2(open.min(close) * (1.0 - walk.unit() * swing / 3.0));
        let volume = 5_000 + (walk.unit() * 120_000.0) as i64;
        last = close;
        json!([candle_time(minute), open, high, low, close, volume])
    };

    let mut rows = Vec::new();
    for day in from.div_euclid(1440)..=to.div_euclid(1440) {
        // 1970-01-01 was a Thursday; 5 and 6 are Saturday and Sunday.
        if (day + 3).rem_euclid(7) >= 5 {
            continue;
        }
        if step == 0 {
            let minute = day * 1440;
            if (from..=to).contains(&minute) {
                rows.push(bar(minute));
            }
            continue;
        }
        let mut minute = day * 1440 + 9 * 60 + 15;
        while minute < day * 1440 + 15 * 60 + 30 {
            if (from..=to).contains(&minute) {
                rows.push(bar(minute));
            }
            minute += step;
        }
    }
    ok(Value::Array(rows))
}

fn search(body: &Value) -> Response {
    let exchange = body["exchange"].as_str().unwrap_or_default();
    let query = body["searchscrip"].as_str().unwrap_or_default().trim().to_ascii_uppercase();
    if exchange.is_empty() || query.is_empty() {
        return broker_error("Invalid exchange or searchscrip", "AB4019");
    }
    let hits: Vec<Value> = fixture(INSTRUMENTS)
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| {
            i["exchange"] == exchange
                && i["tradingsymbol"]
                    .as_str()
                    .is_some_and(|s| s.to_ascii_uppercase().contains(&query))
        })
        .map(|i| {
            json!({
                "exchange": i["exchange"],
                "tradingsymbol": i["tradingsymbol"],
                "symboltoken": i["symboltoken"]
            })
        })
        .collect();
    let message = if hits.is_empty() {
        "Search successful. No matching trading symbols found for the given query.".to_string()
    } else {
        format!("Search successful. Found {} trading symbols for the given query.", hits.len())
    };
    Json(json!({ "status": true, "message": message, "errorcode": "", "data": hits }))
        .into_response()
}

struct OrderFields {
    variety: String,
    symbol: String,
    token: String,
    side: String,
    exchange: String,
    order_type: String,
    product: String,
    duration: String,
    quantity: i64,
    price: f64,
    trigger: f64,
}

/// SmartAPI documents every order field as a string, so anything else is
/// rejected here rather than discovered with a real order.
fn order_fields(body: &Value) -> Result<OrderFields, Response> {
    let text = |key: &str| match body.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        _ => Err(broker_error(&format!("Invalid {key}"), "AB4008")),
    };
    let fields = OrderFields {
        variety: text("variety")?,
        symbol: text("tradingsymbol")?,
        token: text("symboltoken")?,
        side: text("transactiontype")?,
        exchange: text("exchange")?,
        order_type: text("ordertype")?,
        product: text("producttype")?,
        duration: text("duration")?,
        quantity: text("quantity")?.parse().unwrap_or(0),
        price: text("price")?.parse().unwrap_or(-1.0),
        trigger: match body.get("triggerprice") {
            None => 0.0,
            Some(_) => text("triggerprice")?.parse().unwrap_or(-1.0),
        },
    };
    let stop_loss = matches!(fields.order_type.as_str(), "STOPLOSS_LIMIT" | "STOPLOSS_MARKET");
    if !matches!(fields.side.as_str(), "BUY" | "SELL") {
        return Err(broker_error("Invalid transactiontype", "AB4008"));
    }
    if !matches!(fields.order_type.as_str(), "MARKET" | "LIMIT") && !stop_loss {
        return Err(broker_error("Invalid ordertype", "AB4008"));
    }
    if !matches!(fields.product.as_str(), "DELIVERY" | "INTRADAY" | "CARRYFORWARD" | "MARGIN") {
        return Err(broker_error("Invalid Product Type", "AB1012"));
    }
    if stop_loss != (fields.variety == "STOPLOSS") {
        return Err(broker_error("Invalid Order Variety", "AB1008"));
    }
    if fields.quantity <= 0 {
        return Err(broker_error("Invalid quantity", "AB4008"));
    }
    let priced = matches!(fields.order_type.as_str(), "LIMIT" | "STOPLOSS_LIMIT");
    if fields.price < 0.0 || (priced && fields.price == 0.0) {
        return Err(broker_error("Invalid price", "AB4008"));
    }
    if stop_loss && fields.trigger <= 0.0 {
        return Err(broker_error("Invalid triggerprice", "AB4008"));
    }
    Ok(fields)
}

fn is_open(order: &Value) -> bool {
    !matches!(order["status"].as_str(), Some("complete" | "rejected" | "cancelled"))
}

fn place_order(mock: &Mock, token: &str, body: &Value) -> Response {
    let order = match order_fields(body) {
        Ok(order) => order,
        Err(response) => return response,
    };
    let id = format!("2610160009{:05}", mock.next_order.fetch_add(1, Ordering::Relaxed) + 1);
    let unique = format!("6b1d2c3e-mock-4f00-9a01-{}", &id[3..]);
    let ltp = instrument(&order.exchange, &order.token)
        .and_then(|i| i["ltp"].as_f64())
        .unwrap_or(order.price);
    let (status, average, filled) = match order.order_type.as_str() {
        "MARKET" => ("complete", ltp, order.quantity),
        "LIMIT" => ("open", 0.0, 0),
        _ => ("trigger pending", 0.0, 0),
    };
    let entry = json!({
        "variety": order.variety,
        "ordertype": order.order_type,
        "producttype": order.product,
        "duration": order.duration,
        "price": order.price,
        "triggerprice": format!("{:.2}", order.trigger),
        "quantity": order.quantity.to_string(),
        "tradingsymbol": order.symbol,
        "transactiontype": order.side,
        "exchange": order.exchange,
        "symboltoken": order.token,
        "instrumenttype": "",
        "lotsize": "1",
        "averageprice": average,
        "filledshares": filled.to_string(),
        "unfilledshares": (order.quantity - filled).to_string(),
        "orderid": id,
        "text": "",
        "status": status,
        "orderstatus": status,
        "updatetime": "16-Oct-2026 11:00:00",
        "exchtime": "16-Oct-2026 11:00:00",
        "uniqueorderid": unique
    });
    mock.with_book(token, |book| book.insert(0, entry));
    ok(json!({ "script": order.symbol, "orderid": id, "uniqueorderid": unique }))
}

fn modify_order(mock: &Mock, token: &str, body: &Value) -> Response {
    let Some(order_id) = body["orderid"].as_str().filter(|id| !id.is_empty()) else {
        return broker_error("Invalid orderid", "AB4008");
    };
    let order = match order_fields(body) {
        Ok(order) => order,
        Err(response) => return response,
    };
    mock.with_book(token, |book| {
        let Some(entry) = book.iter_mut().find(|o| o["orderid"] == order_id && is_open(o)) else {
            return broker_error("Order not found", "AB1013");
        };
        if entry["variety"] != order.variety.as_str() {
            return broker_error("Invalid Order Variety", "AB1008");
        }
        entry["ordertype"] = json!(order.order_type);
        entry["price"] = json!(order.price);
        entry["triggerprice"] = json!(format!("{:.2}", order.trigger));
        entry["quantity"] = json!(order.quantity.to_string());
        entry["unfilledshares"] = json!(order.quantity.to_string());
        entry["updatetime"] = json!("16-Oct-2026 11:05:00");
        ok(json!({ "orderid": order_id, "uniqueorderid": entry["uniqueorderid"] }))
    })
}

fn cancel_order(mock: &Mock, token: &str, body: &Value) -> Response {
    let (Some(variety), Some(order_id)) = (body["variety"].as_str(), body["orderid"].as_str()) else {
        return broker_error("Invalid variety or orderid", "AB4008");
    };
    mock.with_book(token, |book| {
        let Some(entry) = book.iter_mut().find(|o| o["orderid"] == order_id && is_open(o)) else {
            return broker_error("Order not found", "AB1013");
        };
        if entry["variety"] != variety {
            return broker_error("Invalid Order Variety", "AB1008");
        }
        entry["status"] = json!("cancelled");
        entry["orderstatus"] = json!("cancelled");
        entry["cancelsize"] = entry["unfilledshares"].clone();
        ok(json!({ "orderid": order_id, "uniqueorderid": entry["uniqueorderid"] }))
    })
}

/// Rates are illustrative, close to Angel One's equity schedule.
fn estimate_charges(body: &Value) -> Response {
    let Some(orders) = body["orders"].as_array().filter(|o| !o.is_empty()) else {
        return broker_error("Invalid orders", "AB4008");
    };
    let keys = ["product_type", "transaction_type", "quantity", "price", "exchange", "symbol_name", "token"];
    let (mut brokerage, mut txn, mut stt, mut stamp, mut sebi, mut value) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for order in orders {
        if let Some(bad) = keys.iter().find(|key| !order[**key].is_string()) {
            return broker_error(&format!("Invalid {bad}"), "AB4008");
        }
        let number = |key: &str| order[key].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let trade_value = number("quantity") * number("price");
        let delivery = order["product_type"] == "DELIVERY";
        let buy = order["transaction_type"] == "BUY";
        value += trade_value;
        brokerage += (trade_value * if delivery { 0.001 } else { 0.0003 }).min(20.0);
        txn += trade_value * 0.000_029_7;
        stt += trade_value * if delivery { 0.001 } else if buy { 0.0 } else { 0.000_25 };
        stamp += if buy { trade_value * if delivery { 0.000_15 } else { 0.000_03 } } else { 0.0 };
        sebi += trade_value * 0.000_001;
    }
    let gst = (brokerage + txn + sebi) * 0.18;
    let external = txn + stt + stamp + sebi;
    let item = |name: &str, amount: f64, parts: Value| {
        json!({ "name": name, "amount": round4(amount), "msg": "", "breakup": parts })
    };
    let breakup = json!([
        item("Angel One Brokerage", brokerage, json!([])),
        item("External Charges", external, json!([
            item("Exchange Transaction Charges", txn, json!([])),
            item("Securities Transaction Tax", stt, json!([])),
            item("Stamp Duty", stamp, json!([])),
            item("SEBI Fees", sebi, json!([]))
        ])),
        item("Taxes", gst, json!([item("Goods and Services Tax", gst, json!([]))]))
    ]);
    ok(json!({
        "summary": {
            "total_charges": round4(brokerage + external + gst),
            "trade_value": round4(value),
            "breakup": breakup
        },
        "charges": []
    }))
}
