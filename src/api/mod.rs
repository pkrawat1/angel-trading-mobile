//! Broker calls, exposed to the views as Dioxus server functions.
//!
//! Each client's SmartAPI private key and static-IP proxy stay on the server,
//! read from the web app's store, the same split the web app has with
//! trade_galleon. The device holds the session JWT and sends it with the
//! client code, which tells the server whose key and proxy to use.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::AuthTokens;

mod models;
pub use models::*;

#[cfg(feature = "server")]
mod clients;
#[cfg(feature = "server")]
mod etf;
#[cfg(feature = "server")]
mod plug_crypto;
#[cfg(feature = "server")]
mod smartapi;

/// What the device sends with every call after sign-in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub client_code: String,
    pub jwt: String,
}

#[server]
pub async fn login_with_totp(
    client_code: String,
    password: String,
    totp: String,
) -> Result<AuthTokens, ServerFnError> {
    let body = serde_json::json!({ "clientcode": client_code, "password": password, "totp": totp });
    let data: LoginData = smartapi::login(&client_code, &body).await?;
    Ok(AuthTokens {
        jwt_token: data.jwt_token,
        refresh_token: data.refresh_token,
        feed_token: data.feed_token,
        user_id: client_code,
    })
}

#[server]
pub async fn logout_session(session: Session) -> Result<(), ServerFnError> {
    let body = serde_json::json!({ "clientcode": session.client_code });
    smartapi::post::<serde_json::Value>("rest/secure/angelbroking/user/v1/logout", &session, &body)
        .await?;
    Ok(())
}

#[server]
pub async fn get_profile(session: Session) -> Result<Profile, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/user/v1/getProfile", &session).await
}

#[server]
pub async fn get_funds(session: Session) -> Result<Funds, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/user/v1/getRMS", &session).await
}

#[server]
pub async fn get_holdings(session: Session) -> Result<Vec<Holding>, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/portfolio/v1/getHolding", &session).await
}

#[server]
pub async fn get_order_book(session: Session) -> Result<Vec<Order>, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/order/v1/getOrderBook", &session).await
}

#[server]
pub async fn get_trade_book(session: Session) -> Result<Vec<Trade>, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/order/v1/getTradeBook", &session).await
}

#[server]
pub async fn get_positions(session: Session) -> Result<Vec<Position>, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/order/v1/getPosition", &session).await
}

/// Quotes in FULL mode: OHLC, circuits, 52-week range and market depth.
///
/// Callers must not pass an empty list: the URL-encoded arguments then carry no
/// `instruments` key and the call fails before reaching this body.
#[server]
pub async fn get_quotes(
    session: Session,
    instruments: Vec<Instrument>,
) -> Result<Vec<Quote>, ServerFnError> {
    let mut quotes = Vec::with_capacity(instruments.len());
    // The quote endpoint takes at most 50 tokens per call.
    for chunk in instruments.chunks(50) {
        let mut exchange_tokens = std::collections::BTreeMap::<&str, Vec<&str>>::new();
        for instrument in chunk {
            exchange_tokens
                .entry(instrument.exchange.as_str())
                .or_default()
                .push(instrument.symbol_token.as_str());
        }
        let body = serde_json::json!({ "mode": "FULL", "exchangeTokens": exchange_tokens });
        let batch: QuoteBatch =
            smartapi::post("rest/secure/angelbroking/market/v1/quote", &session, &body).await?;
        quotes.extend(batch.fetched);
    }
    Ok(quotes)
}

/// Candles for the trailing `days`, clamped to the longest range SmartAPI
/// serves for `interval` ("ONE_MINUTE", "FIVE_MINUTE", "FIFTEEN_MINUTE",
/// "ONE_HOUR", "ONE_DAY", ...).
#[server]
pub async fn get_candles(
    session: Session,
    exchange: String,
    symbol_token: String,
    interval: String,
    days: u32,
) -> Result<Vec<Candle>, ServerFnError> {
    let max_days = match interval.as_str() {
        "ONE_MINUTE" => 30,
        "THREE_MINUTE" => 60,
        "FIVE_MINUTE" | "TEN_MINUTE" => 100,
        "FIFTEEN_MINUTE" | "THIRTY_MINUTE" => 200,
        "ONE_HOUR" => 400,
        _ => 2000,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let from = now - i64::from(days.min(max_days)) * 86_400;
    let body = serde_json::json!({
        "exchange": exchange,
        "symboltoken": symbol_token,
        "interval": interval,
        "fromdate": smartapi::ist_minute(from),
        "todate": smartapi::ist_minute(now),
    });
    let rows: Vec<Vec<serde_json::Value>> = smartapi::post(
        "rest/secure/angelbroking/historical/v1/getCandleData",
        &session,
        &body,
    )
    .await?;
    Ok(rows.iter().filter_map(|row| Candle::from_row(row)).collect())
}

/// Instruments on `exchange` ("NSE", "BSE", "NFO", ...) whose symbol matches
/// `query`. SmartAPI allows one search per second, so callers should debounce.
#[server]
pub async fn search_scrip(
    session: Session,
    exchange: String,
    query: String,
) -> Result<Vec<Instrument>, ServerFnError> {
    let body = serde_json::json!({ "exchange": exchange, "searchscrip": query });
    smartapi::post("rest/secure/angelbroking/order/v1/searchScrip", &session, &body).await
}

#[server]
pub async fn place_order(session: Session, order: OrderRequest) -> Result<OrderAck, ServerFnError> {
    smartapi::post_once(
        "rest/secure/angelbroking/order/v1/placeOrder",
        &session,
        &order.broker_payload(),
    )
    .await
}

/// `order.order_id` must name the order being modified.
#[server]
pub async fn modify_order(session: Session, order: OrderRequest) -> Result<OrderAck, ServerFnError> {
    if order.order_id.as_deref().unwrap_or_default().is_empty() {
        return Err(ServerFnError::ServerError("Missing the order id to modify".into()));
    }
    smartapi::post_once(
        "rest/secure/angelbroking/order/v1/modifyOrder",
        &session,
        &order.broker_payload(),
    )
    .await
}

#[server]
pub async fn cancel_order(
    session: Session,
    variety: String,
    order_id: String,
) -> Result<OrderAck, ServerFnError> {
    let body = serde_json::json!({ "variety": variety, "orderid": order_id });
    smartapi::post_once("rest/secure/angelbroking/order/v1/cancelOrder", &session, &body).await
}

/// Brokerage and taxes for `order`. For MARKET orders, pass the LTP as `price`.
#[server]
pub async fn estimate_charges(session: Session, order: OrderRequest) -> Result<Charges, ServerFnError> {
    let body = serde_json::json!({ "orders": [{
        "product_type": order.product_type,
        "transaction_type": order.transaction_type,
        "quantity": order.quantity.to_string(),
        "price": format!("{:.2}", order.price),
        "exchange": order.exchange,
        "symbol_name": order.trading_symbol,
        "token": order.symbol_token,
    }]});
    let data: serde_json::Value = smartapi::post(
        "rest/secure/angelbroking/brokerage/v1/estimateCharges",
        &session,
        &body,
    )
    .await?;
    Ok(Charges::from_summary(&data["summary"]))
}

#[cfg(all(test, feature = "server"))]
mod tests;
