//! Broker calls, exposed to the views as Dioxus server functions.
//!
//! The SmartAPI private key and the static-IP proxy stay on the server, the
//! same split the web app has with trade_galleon. The device holds only the
//! session JWT and passes it with each call.

use dioxus::prelude::*;

use crate::auth::AuthTokens;

mod models;
pub use models::*;

#[cfg(feature = "server")]
mod smartapi;

#[server]
pub async fn login_with_totp(
    client_code: String,
    password: String,
    totp: String,
) -> Result<AuthTokens, ServerFnError> {
    let body = serde_json::json!({ "clientcode": client_code, "password": password, "totp": totp });
    let data: LoginData =
        smartapi::post("rest/auth/angelbroking/user/v1/loginByPassword", None, &body).await?;
    Ok(AuthTokens {
        jwt_token: data.jwt_token,
        refresh_token: data.refresh_token,
        feed_token: data.feed_token,
        user_id: client_code,
    })
}

#[server]
pub async fn logout_session(token: String, client_code: String) -> Result<(), ServerFnError> {
    let body = serde_json::json!({ "clientcode": client_code });
    smartapi::post::<serde_json::Value>("rest/secure/angelbroking/user/v1/logout", Some(&token), &body)
        .await?;
    Ok(())
}

#[server]
pub async fn get_profile(token: String) -> Result<Profile, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/user/v1/getProfile", &token).await
}

#[server]
pub async fn get_funds(token: String) -> Result<Funds, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/user/v1/getRMS", &token).await
}

#[server]
pub async fn get_holdings(token: String) -> Result<Vec<Holding>, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/portfolio/v1/getHolding", &token).await
}

#[server]
pub async fn get_order_book(token: String) -> Result<Vec<Order>, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/order/v1/getOrderBook", &token).await
}

#[server]
pub async fn get_trade_book(token: String) -> Result<Vec<Trade>, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/order/v1/getTradeBook", &token).await
}

#[server]
pub async fn get_positions(token: String) -> Result<Vec<Position>, ServerFnError> {
    smartapi::get("rest/secure/angelbroking/order/v1/getPosition", &token).await
}

/// Quotes in FULL mode: OHLC, circuits, 52-week range and market depth.
///
/// Callers must not pass an empty list: the URL-encoded arguments then carry no
/// `instruments` key and the call fails before reaching this body.
#[server]
pub async fn get_quotes(
    token: String,
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
            smartapi::post("rest/secure/angelbroking/market/v1/quote", Some(&token), &body).await?;
        quotes.extend(batch.fetched);
    }
    Ok(quotes)
}

/// Candles for the trailing `days`, clamped to the longest range SmartAPI
/// serves for `interval` ("ONE_MINUTE", "FIVE_MINUTE", "FIFTEEN_MINUTE",
/// "ONE_HOUR", "ONE_DAY", ...).
#[server]
pub async fn get_candles(
    token: String,
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
        Some(&token),
        &body,
    )
    .await?;
    Ok(rows.iter().filter_map(|row| Candle::from_row(row)).collect())
}

/// Instruments on `exchange` ("NSE", "BSE", "NFO", ...) whose symbol matches
/// `query`. SmartAPI allows one search per second, so callers should debounce.
#[server]
pub async fn search_scrip(
    token: String,
    exchange: String,
    query: String,
) -> Result<Vec<Instrument>, ServerFnError> {
    let body = serde_json::json!({ "exchange": exchange, "searchscrip": query });
    smartapi::post("rest/secure/angelbroking/order/v1/searchScrip", Some(&token), &body).await
}

#[server]
pub async fn place_order(token: String, order: OrderRequest) -> Result<OrderAck, ServerFnError> {
    smartapi::post_once(
        "rest/secure/angelbroking/order/v1/placeOrder",
        &token,
        &order.broker_payload(),
    )
    .await
}

/// `order.order_id` must name the order being modified.
#[server]
pub async fn modify_order(token: String, order: OrderRequest) -> Result<OrderAck, ServerFnError> {
    if order.order_id.as_deref().unwrap_or_default().is_empty() {
        return Err(ServerFnError::ServerError("Missing the order id to modify".into()));
    }
    smartapi::post_once(
        "rest/secure/angelbroking/order/v1/modifyOrder",
        &token,
        &order.broker_payload(),
    )
    .await
}

#[server]
pub async fn cancel_order(
    token: String,
    variety: String,
    order_id: String,
) -> Result<OrderAck, ServerFnError> {
    let body = serde_json::json!({ "variety": variety, "orderid": order_id });
    smartapi::post_once("rest/secure/angelbroking/order/v1/cancelOrder", &token, &body).await
}

/// Brokerage and taxes for `order`. For MARKET orders, pass the LTP as `price`.
#[server]
pub async fn estimate_charges(token: String, order: OrderRequest) -> Result<Charges, ServerFnError> {
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
        Some(&token),
        &body,
    )
    .await?;
    Ok(Charges::from_summary(&data["summary"]))
}

#[cfg(all(test, feature = "server"))]
mod tests;
