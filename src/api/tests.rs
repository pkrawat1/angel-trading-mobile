//! Every server function against the SmartAPI stand-in in
//! examples/mock_smartapi: what the app sends, how it reads each response,
//! and how it handles broker errors, session expiry and retries.
//!
//! Each test uses its own session token, so the tests can share one mock and
//! read back only their own requests.

#[path = "../../examples/mock_smartapi/server.rs"]
mod mock;

use std::fmt::Debug;
use std::sync::{Arc, OnceLock};

use axum::http::Method;
use dioxus::prelude::ServerFnError;
use serde_json::json;

use super::clients::{self, ClientConfig};
use super::*;
use mock::{Mock, Recorded};

/// The mock picks a scenario from the JWT alone, so a test names its session
/// by the token.
impl From<&str> for Session {
    fn from(jwt: &str) -> Self {
        Session { client_code: "MOCK".into(), jwt: jwt.into() }
    }
}

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime")
    })
}

/// The HTTP client is a process-wide singleton whose pooled connections belong
/// to the runtime that opened them, so every test runs on this one runtime
/// rather than its own.
fn mock() -> &'static Arc<Mock> {
    static MOCK: OnceLock<Arc<Mock>> = OnceLock::new();
    MOCK.get_or_init(|| {
        let (addr, mock) = rt()
            .block_on(mock::serve(([127, 0, 0, 1], 0).into()))
            .expect("mock SmartAPI");
        smartapi::TEST_BASE_URL
            .set(format!("http://{addr}/"))
            .expect("base URL set once");
        mock
    })
}

fn run<T>(call: impl std::future::Future<Output = T>) -> T {
    mock();
    rt().block_on(call)
}

#[track_caller]
fn only(requests: Vec<Recorded>) -> Recorded {
    assert_eq!(requests.len(), 1, "expected one request, got {requests:#?}");
    requests.into_iter().next().unwrap()
}

#[track_caller]
fn message<T: Debug>(result: Result<T, ServerFnError>) -> String {
    match result {
        Err(ServerFnError::ServerError(message)) => message,
        other => panic!("expected a server error, got {other:?}"),
    }
}

#[track_caller]
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

fn instrument(exchange: &str, symbol: &str, token: &str) -> Instrument {
    Instrument {
        exchange: exchange.into(),
        trading_symbol: symbol.into(),
        symbol_token: token.into(),
    }
}

fn limit_buy() -> OrderRequest {
    OrderRequest {
        order_id: None,
        variety: "NORMAL".into(),
        exchange: "NSE".into(),
        trading_symbol: "SBIN-EQ".into(),
        symbol_token: "3045".into(),
        transaction_type: "BUY".into(),
        order_type: "LIMIT".into(),
        product_type: "DELIVERY".into(),
        duration: "DAY".into(),
        price: 798.5,
        trigger_price: 0.0,
        quantity: 10,
    }
}

fn find<'a>(orders: &'a [Order], order_id: &str) -> &'a Order {
    orders
        .iter()
        .find(|o| o.order_id == order_id)
        .unwrap_or_else(|| panic!("order {order_id} not in the book"))
}

#[test]
fn login_sends_credentials_without_a_session() {
    let mock = mock();
    let tokens = run(login_with_totp("Login1".into(), "secret".into(), "123456".into())).unwrap();
    assert_eq!(tokens.jwt_token, "login1-session");
    assert_eq!(tokens.refresh_token, "mock-refresh-token");
    assert_eq!(tokens.feed_token, "mock-feed-token");
    assert_eq!(tokens.user_id, "Login1");

    let request = only(
        mock.requests()
            .into_iter()
            .filter(|r| r.body["clientcode"] == "Login1")
            .collect(),
    );
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.path, "/rest/auth/angelbroking/user/v1/loginByPassword");
    assert_eq!(
        request.body,
        json!({ "clientcode": "Login1", "password": "secret", "totp": "123456" })
    );
    assert_eq!(request.header("authorization"), None);
}

#[test]
fn login_reports_a_rejected_totp() {
    let result = run(login_with_totp("Login2".into(), "secret".into(), "000000".into()));
    assert_eq!(message(result), "Invalid totp [AB1050]");
}

#[test]
fn logout_posts_the_client_code() {
    let mock = mock();
    run(logout_session(Session { client_code: "MOCK123".into(), jwt: "logout".into() })).unwrap();
    let request = only(mock.requests_for("logout"));
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.path, "/rest/secure/angelbroking/user/v1/logout");
    assert_eq!(request.body, json!({ "clientcode": "MOCK123" }));
}

#[test]
fn secure_calls_send_the_session_and_app_headers() {
    let mock = mock();
    run(get_profile("headers".into())).unwrap();
    let request = only(mock.requests_for("headers"));
    assert_eq!(request.method, Method::GET);
    assert_eq!(request.header("authorization"), Some("Bearer headers"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("x-usertype"), Some("USER"));
    assert_eq!(request.header("x-sourceid"), Some("WEB"));
    // Values come from the server's environment; Angel One wants the keys
    // present either way.
    for name in ["x-clientlocalip", "x-clientpublicip", "x-macaddress", "x-privatekey"] {
        assert!(request.headers.contains_key(name), "missing {name}");
    }
}

#[test]
fn reads_use_get_on_their_documented_paths() {
    let mock = mock();
    let token = || Session::from("paths");
    run(get_profile(token())).unwrap();
    run(get_funds(token())).unwrap();
    run(get_holdings(token())).unwrap();
    run(get_order_book(token())).unwrap();
    run(get_trade_book(token())).unwrap();
    run(get_positions(token())).unwrap();
    let calls: Vec<(Method, String)> = mock
        .requests_for("paths")
        .into_iter()
        .map(|r| (r.method, r.path))
        .collect();
    let expected = [
        "/rest/secure/angelbroking/user/v1/getProfile",
        "/rest/secure/angelbroking/user/v1/getRMS",
        "/rest/secure/angelbroking/portfolio/v1/getHolding",
        "/rest/secure/angelbroking/order/v1/getOrderBook",
        "/rest/secure/angelbroking/order/v1/getTradeBook",
        "/rest/secure/angelbroking/order/v1/getPosition",
    ]
    .map(|path| (Method::GET, path.to_string()));
    assert_eq!(calls, expected);
}

#[test]
fn profile() {
    let profile = run(get_profile("profile".into())).unwrap();
    assert_eq!(profile.client_code, "MOCK123");
    assert_eq!(profile.name, "Mock Trader");
    assert_eq!(profile.broker_id, "B2C");
    assert_eq!(profile.email, "");
}

#[test]
fn funds_read_string_amounts_and_nulls() {
    let funds = run(get_funds("funds".into())).unwrap();
    assert_eq!(funds.net, 125432.75);
    assert_eq!(funds.available_cash, 118056.0);
    assert_eq!(funds.m2m_unrealized, -33.0);
    assert_eq!(funds.m2m_realized, 142.5);
    assert_eq!(funds.utilised_debits, 7376.75);
    assert_eq!(funds.utilised_holding_sales, 4051.0);
    assert_eq!(funds.utilised_span, 0.0, "null");
    assert_eq!(funds.utilised_payout, 0.0, "\"-0.00\"");
}

#[test]
fn holdings() {
    let holdings = run(get_holdings("holdings".into())).unwrap();
    let symbols: Vec<&str> = holdings.iter().map(|h| h.trading_symbol.as_str()).collect();
    assert_eq!(
        symbols,
        ["SBIN-EQ", "RELIANCE-EQ", "TATASTEEL-EQ", "INFY-EQ", "ITC-EQ", "YESBANK-EQ"]
    );
    let sbin = &holdings[0];
    assert_eq!(
        (sbin.quantity, sbin.average_price, sbin.ltp, sbin.close),
        (20, 742.35, 812.6, 805.1)
    );
    assert_eq!(holdings[4].t1_quantity, 40);
    // Sold out, and the broker sends a null close.
    assert_eq!((holdings[5].quantity, holdings[5].close), (0, 0.0));
    assert!(!holdings[5].is_active());
}

#[test]
fn order_book() {
    let orders = run(get_order_book("order-book".into())).unwrap();
    assert_eq!(orders.len(), 5);
    let stop_loss = find(&orders, "261016000123402");
    assert_eq!(
        (stop_loss.quantity, stop_loss.price, stop_loss.trigger_price),
        (50, 150.0, 150.5)
    );
    assert_eq!(stop_loss.variety, "STOPLOSS");
    assert_eq!(stop_loss.status, "trigger pending");
    let open: Vec<&str> = orders
        .iter()
        .filter(|o| o.is_open())
        .map(|o| o.order_id.as_str())
        .collect();
    assert_eq!(open, ["261016000123401", "261016000123402"]);
    let filled = find(&orders, "261016000123390");
    assert_eq!((filled.average_price, filled.filled_shares), (1475.35, 5));
    assert!(!find(&orders, "261016000123385").text.is_empty(), "rejection reason");
}

#[test]
fn trade_book() {
    let trades = run(get_trade_book("trade-book".into())).unwrap();
    assert_eq!(trades.len(), 2);
    let fill = &trades[0];
    assert_eq!(
        (fill.fill_price, fill.fill_size, fill.trade_value),
        (1475.35, 5, 7376.75)
    );
    assert_eq!(
        (fill.order_id.as_str(), fill.fill_id.as_str(), fill.fill_time.as_str()),
        ("261016000123390", "50123456", "09:41:27")
    );
}

#[test]
fn positions() {
    let positions = run(get_positions("positions".into())).unwrap();
    let [intraday, closed, option] = positions.as_slice() else {
        panic!("expected three positions, got {positions:#?}");
    };
    assert_eq!((intraday.net_qty, intraday.ltp), (5, 1468.75));
    near(intraday.pnl(intraday.ltp), -33.0);
    // Squared off: no LTP is sent, and the P&L is all realised.
    assert_eq!((closed.net_qty, closed.ltp), (0, 0.0));
    near(closed.pnl(0.0), 142.5);
    assert_eq!((option.net_qty, option.lot_size), (-75, 75));
    near(option.pnl(182.35), -817.5);
}

#[test]
fn quotes_ask_for_full_mode_grouped_by_exchange() {
    let mock = mock();
    let wanted = vec![
        instrument("NSE", "SBIN-EQ", "3045"),
        instrument("BSE", "SBIN", "500112"),
        instrument("NSE", "Nifty 50", "99926000"),
    ];
    let quotes = run(get_quotes("quotes".into(), wanted)).unwrap();
    let request = only(mock.requests_for("quotes"));
    assert_eq!(request.path, "/rest/secure/angelbroking/market/v1/quote");
    assert_eq!(
        request.body,
        json!({
            "mode": "FULL",
            "exchangeTokens": { "BSE": ["500112"], "NSE": ["3045", "99926000"] }
        })
    );
    assert_eq!(quotes.len(), 3);

    let sbin = quotes.iter().find(|q| q.symbol_token == "3045").unwrap();
    assert_eq!(sbin.instrument(), instrument("NSE", "SBIN-EQ", "3045"));
    assert_eq!(
        (sbin.ltp, sbin.open, sbin.high, sbin.low, sbin.close),
        (812.6, 806.0, 815.4, 803.25, 805.1)
    );
    assert_eq!((sbin.net_change, sbin.percent_change, sbin.avg_price), (7.5, 0.93, 810.22));
    assert_eq!((sbin.lower_circuit, sbin.upper_circuit), (724.6, 885.6));
    assert_eq!((sbin.week_52_low, sbin.week_52_high), (680.0, 912.1));
    assert_eq!(
        (sbin.trade_volume, sbin.last_trade_qty, sbin.total_buy_qty, sbin.total_sell_qty),
        (11234567, 25, 512340, 689021)
    );
    assert_eq!(sbin.exch_feed_time, "16-Oct-2026 15:29:59");
    assert_eq!((sbin.depth.buy.len(), sbin.depth.sell.len()), (5, 5));
    assert_eq!(
        sbin.depth.buy[0],
        DepthLevel { price: 812.55, quantity: 145, orders: 3 }
    );

    // Indices have no order book, and SmartAPI leaves the depth key out.
    let nifty = quotes.iter().find(|q| q.symbol_token == "99926000").unwrap();
    assert!(nifty.depth.buy.is_empty() && nifty.depth.sell.is_empty());
}

#[test]
fn quotes_split_into_batches_of_fifty() {
    let mock = mock();
    let known: Vec<Instrument> = serde_json::from_str(mock::INSTRUMENTS).unwrap();
    let wanted: Vec<Instrument> = known.iter().cycle().take(60).cloned().collect();
    let quotes = run(get_quotes("quote-batches".into(), wanted)).unwrap();
    assert_eq!(quotes.len(), 60);
    let sizes: Vec<usize> = mock
        .requests_for("quote-batches")
        .iter()
        .map(|r| {
            let tokens = r.body["exchangeTokens"].as_object().unwrap();
            tokens.values().map(|t| t.as_array().unwrap().len()).sum()
        })
        .collect();
    assert_eq!(sizes, [50, 10]);
}

#[test]
fn quotes_drop_tokens_the_broker_could_not_fetch() {
    let wanted = vec![instrument("NSE", "SBIN-EQ", "3045"), instrument("NSE", "GONE-EQ", "1")];
    let quotes = run(get_quotes("quote-unfetched".into(), wanted)).unwrap();
    let tokens: Vec<&str> = quotes.iter().map(|q| q.symbol_token.as_str()).collect();
    assert_eq!(tokens, ["3045"]);
}

#[test]
fn candles_cover_the_trailing_days_in_ist() {
    let mock = mock();
    let candles = run(get_candles(
        "candles".into(),
        "NSE".into(),
        "3045".into(),
        "FIVE_MINUTE".into(),
        5,
    ))
    .unwrap();
    let request = only(mock.requests_for("candles"));
    assert_eq!(request.path, "/rest/secure/angelbroking/historical/v1/getCandleData");
    assert_eq!(
        (&request.body["exchange"], &request.body["symboltoken"], &request.body["interval"]),
        (&json!("NSE"), &json!("3045"), &json!("FIVE_MINUTE"))
    );
    let minute = |key: &str| {
        let text = request.body[key].as_str().unwrap();
        mock::parse_ist_minute(text).unwrap_or_else(|| panic!("{key} {text:?}"))
    };
    assert_eq!(minute("todate") - minute("fromdate"), 5 * 1440);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let now_ist = (now + 19_800) / 60;
    assert!((minute("todate") - now_ist).abs() <= 1, "todate is now in IST");

    assert!(!candles.is_empty());
    for candle in &candles {
        assert!(candle.time.ends_with("+05:30"), "{candle:?}");
        let body = candle.open.min(candle.close)..=candle.open.max(candle.close);
        assert!(candle.low <= *body.start() && candle.high >= *body.end(), "{candle:?}");
        assert!(candle.volume > 0, "{candle:?}");
    }
}

#[test]
fn candle_ranges_are_clamped_to_the_interval_limit() {
    let mock = mock();
    let limits = [
        ("ONE_MINUTE", 30),
        ("THREE_MINUTE", 60),
        ("FIVE_MINUTE", 100),
        ("TEN_MINUTE", 100),
        ("FIFTEEN_MINUTE", 200),
        ("THIRTY_MINUTE", 200),
        ("ONE_HOUR", 400),
        ("ONE_DAY", 2000),
    ];
    for (interval, max_days) in limits {
        let token = format!("clamp-{interval}");
        let call = get_candles(token.as_str().into(), "NSE".into(), "3045".into(), interval.into(), 10_000);
        let candles = run(call).unwrap_or_else(|e| panic!("{interval}: {e}"));
        assert!(!candles.is_empty(), "{interval}");
        let request = only(mock.requests_for(&token));
        let minute = |key: &str| mock::parse_ist_minute(request.body[key].as_str().unwrap()).unwrap();
        assert_eq!(minute("todate") - minute("fromdate"), max_days * 1440, "{interval}");
    }
}

#[test]
fn candle_errors_carry_the_broker_message() {
    let call = get_candles(
        "candles-bad".into(),
        "NSE".into(),
        "3045".into(),
        "TWO_MINUTE".into(),
        5,
    );
    assert_eq!(message(run(call)), "Invalid interval [AB13000]");
}

#[test]
fn search_finds_symbols_on_one_exchange() {
    let mock = mock();
    let hits = run(search_scrip("search".into(), "NSE".into(), "sbin".into())).unwrap();
    assert_eq!(
        hits,
        [instrument("NSE", "SBIN-EQ", "3045"), instrument("NSE", "SBIN-BE", "4884")]
    );
    let request = only(mock.requests_for("search"));
    assert_eq!(request.path, "/rest/secure/angelbroking/order/v1/searchScrip");
    assert_eq!(request.body, json!({ "exchange": "NSE", "searchscrip": "sbin" }));

    let none = run(search_scrip("search-none".into(), "NSE".into(), "ZZZZ".into())).unwrap();
    assert!(none.is_empty());
}

#[test]
fn place_order_sends_every_number_as_a_string() {
    let mock = mock();
    let ack = run(place_order("place".into(), limit_buy())).unwrap();
    assert!(!ack.order_id.is_empty() && !ack.unique_order_id.is_empty(), "{ack:?}");
    assert_eq!(ack.script, "SBIN-EQ");
    let request = only(mock.requests_for("place"));
    assert_eq!(request.path, "/rest/secure/angelbroking/order/v1/placeOrder");
    assert_eq!(
        request.body,
        json!({
            "variety": "NORMAL",
            "exchange": "NSE",
            "tradingsymbol": "SBIN-EQ",
            "symboltoken": "3045",
            "transactiontype": "BUY",
            "ordertype": "LIMIT",
            "producttype": "DELIVERY",
            "duration": "DAY",
            "price": "798.50",
            "triggerprice": "0.00",
            "quantity": "10",
            "squareoff": "0",
            "stoploss": "0"
        })
    );

    let book = run(get_order_book("place".into())).unwrap();
    assert_eq!(book[0].order_id, ack.order_id, "new orders head the book");
    assert_eq!(
        (book[0].status.as_str(), book[0].price, book[0].quantity),
        ("open", 798.5, 10)
    );
    assert!(book[0].is_open());
}

#[test]
fn market_orders_fill_at_the_last_price() {
    let order = OrderRequest { order_type: "MARKET".into(), price: 0.0, ..limit_buy() };
    let ack = run(place_order("market".into(), order)).unwrap();
    let book = run(get_order_book("market".into())).unwrap();
    let placed = find(&book, &ack.order_id);
    assert_eq!(
        (placed.status.as_str(), placed.average_price, placed.filled_shares),
        ("complete", 812.6, 10)
    );
    assert!(!placed.is_open());
}

#[test]
fn stop_loss_orders_need_the_stoploss_variety() {
    let order = OrderRequest {
        transaction_type: "SELL".into(),
        order_type: "STOPLOSS_LIMIT".into(),
        price: 790.0,
        trigger_price: 791.0,
        ..limit_buy()
    };
    let rejected = run(place_order("sl-normal".into(), order.clone()));
    assert_eq!(message(rejected), "Invalid Order Variety [AB1008]");

    let order = OrderRequest { variety: "STOPLOSS".into(), ..order };
    let ack = run(place_order("sl".into(), order)).unwrap();
    let book = run(get_order_book("sl".into())).unwrap();
    let placed = find(&book, &ack.order_id);
    assert_eq!(
        (placed.status.as_str(), placed.trigger_price),
        ("trigger pending", 791.0)
    );
}

#[test]
fn order_rejections_carry_the_broker_message() {
    let no_quantity = OrderRequest { quantity: 0, ..limit_buy() };
    assert_eq!(
        message(run(place_order("bad-qty".into(), no_quantity))),
        "Invalid quantity [AB4008]"
    );
    let unpriced_limit = OrderRequest { price: 0.0, ..limit_buy() };
    assert_eq!(
        message(run(place_order("bad-price".into(), unpriced_limit))),
        "Invalid price [AB4008]"
    );
}

#[test]
fn modify_order_changes_an_open_order() {
    let mock = mock();
    let order = OrderRequest {
        order_id: Some("261016000123401".into()),
        price: 799.5,
        quantity: 12,
        ..limit_buy()
    };
    let ack = run(modify_order("modify".into(), order)).unwrap();
    assert_eq!(ack.order_id, "261016000123401");
    let request = only(mock.requests_for("modify"));
    assert_eq!(request.path, "/rest/secure/angelbroking/order/v1/modifyOrder");
    assert_eq!(
        (&request.body["orderid"], &request.body["price"], &request.body["quantity"]),
        (&json!("261016000123401"), &json!("799.50"), &json!("12"))
    );

    let book = run(get_order_book("modify".into())).unwrap();
    let modified = find(&book, "261016000123401");
    assert_eq!((modified.price, modified.quantity), (799.5, 12));
}

#[test]
fn modify_order_needs_an_order_id() {
    let mock = mock();
    for order_id in [None, Some(String::new())] {
        let order = OrderRequest { order_id, ..limit_buy() };
        let result = run(modify_order("modify-no-id".into(), order));
        assert_eq!(message(result), "Missing the order id to modify");
    }
    assert!(mock.requests_for("modify-no-id").is_empty(), "nothing sent");
}

#[test]
fn modify_order_rejects_a_finished_order() {
    let order = OrderRequest { order_id: Some("261016000123390".into()), ..limit_buy() };
    let result = run(modify_order("modify-done".into(), order));
    assert_eq!(message(result), "Order not found [AB1013]");
}

#[test]
fn cancel_order_sends_the_variety_and_id() {
    let mock = mock();
    let cancel = || cancel_order("cancel".into(), "STOPLOSS".into(), "261016000123402".into());
    let ack = run(cancel()).unwrap();
    assert_eq!(ack.order_id, "261016000123402");
    let request = only(mock.requests_for("cancel"));
    assert_eq!(request.path, "/rest/secure/angelbroking/order/v1/cancelOrder");
    assert_eq!(
        request.body,
        json!({ "variety": "STOPLOSS", "orderid": "261016000123402" })
    );

    let book = run(get_order_book("cancel".into())).unwrap();
    let cancelled = find(&book, "261016000123402");
    assert_eq!(cancelled.status, "cancelled");
    assert!(!cancelled.is_open());
    assert_eq!(message(run(cancel())), "Order not found [AB1013]");
}

#[test]
fn estimate_charges_reads_the_summary() {
    let mock = mock();
    let order = OrderRequest { price: 812.6, ..limit_buy() };
    let charges = run(estimate_charges("charges".into(), order)).unwrap();
    near(charges.trade_value, 8126.0);
    assert!(charges.total_charges > 0.0);
    let names: Vec<&str> = charges.breakup.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Angel One Brokerage", "External Charges", "Taxes"]);
    let parts: f64 = charges.breakup.iter().map(|c| c.amount).sum();
    assert!((parts - charges.total_charges).abs() < 0.001, "{charges:?}");

    let request = only(mock.requests_for("charges"));
    assert_eq!(request.path, "/rest/secure/angelbroking/brokerage/v1/estimateCharges");
    assert_eq!(
        request.body,
        json!({ "orders": [{
            "product_type": "DELIVERY",
            "transaction_type": "BUY",
            "quantity": "10",
            "price": "812.60",
            "exchange": "NSE",
            "symbol_name": "SBIN-EQ",
            "token": "3045"
        }]})
    );
}

#[test]
fn expired_sessions_are_recognised() {
    let err = run(get_order_book("expired-orders".into())).unwrap_err();
    assert!(crate::util::is_session_error(&err), "{err}");
    assert_eq!(message::<()>(Err(err)), "Invalid Token [AG8001]");
}

#[test]
fn broker_errors_keep_their_code() {
    let err = run(get_positions("broker-down-positions".into())).unwrap_err();
    assert!(!crate::util::is_session_error(&err), "{err}");
    assert_eq!(
        message::<()>(Err(err)),
        "Something Went Wrong, Please Try After Sometime [AB1004]"
    );
}

#[test]
fn reads_retry_transient_failures() {
    let mock = mock();
    assert_eq!(run(get_holdings("flaky-holdings".into())).unwrap().len(), 6);
    assert_eq!(mock.requests_for("flaky-holdings").len(), 2);
    run(get_profile("rate-limited-profile".into())).unwrap();
    assert_eq!(mock.requests_for("rate-limited-profile").len(), 2);
}

#[test]
fn reads_give_up_after_three_attempts() {
    let mock = mock();
    let result = run(get_funds("html-funds".into()));
    assert_eq!(
        message(result),
        "Angel One returned an unexpected response (HTTP 502 Bad Gateway)"
    );
    assert_eq!(mock.requests_for("html-funds").len(), 3);
}

/// A retry after a timeout could place the same order twice.
#[test]
fn order_calls_are_never_retried() {
    let mock = mock();
    let placed = run(place_order("flaky-place".into(), limit_buy()));
    assert_eq!(message(placed), "Internal Error, Please try after sometime [AB2001]");
    assert_eq!(mock.requests_for("flaky-place").len(), 1);

    let modify = OrderRequest { order_id: Some("261016000123401".into()), ..limit_buy() };
    let modified = run(modify_order("rate-limited-modify".into(), modify));
    assert_eq!(
        message(modified),
        "Access denied because of exceeding access rate (HTTP 403 Forbidden)"
    );
    assert_eq!(mock.requests_for("rate-limited-modify").len(), 1);

    let cancelled = run(cancel_order(
        "html-cancel".into(),
        "NORMAL".into(),
        "261016000123401".into(),
    ));
    assert_eq!(
        message(cancelled),
        "Angel One returned an unexpected response (HTTP 502 Bad Gateway)"
    );
    assert_eq!(mock.requests_for("html-cancel").len(), 1);
}

#[test]
fn null_data_reads_as_empty() {
    let token = || Session::from("empty-everything");
    assert_eq!(run(get_profile(token())).unwrap(), Profile::default());
    assert_eq!(run(get_funds(token())).unwrap(), Funds::default());
    assert!(run(get_holdings(token())).unwrap().is_empty());
    assert!(run(get_order_book(token())).unwrap().is_empty());
    assert!(run(get_trade_book(token())).unwrap().is_empty());
    assert!(run(get_positions(token())).unwrap().is_empty());
    let quotes = run(get_quotes(token(), vec![instrument("NSE", "SBIN-EQ", "3045")]));
    assert!(quotes.unwrap().is_empty());
    let candles = run(get_candles(token(), "NSE".into(), "3045".into(), "ONE_DAY".into(), 30));
    assert!(candles.unwrap().is_empty());
    assert!(run(search_scrip(token(), "NSE".into(), "SBIN".into())).unwrap().is_empty());
    assert_eq!(run(estimate_charges(token(), limit_buy())).unwrap(), Charges::default());
}

/// server_fn sends arguments URL-encoded and decodes them with these settings
/// (server_fn 0.6, codec/url.rs).
fn over_the_wire<T: serde::Serialize + serde::de::DeserializeOwned>(
    args: &T,
) -> Result<T, serde_qs::Error> {
    let encoded = serde_qs::to_string(args)?;
    serde_qs::Config::new(5, false).deserialize_str(&encoded)
}

#[test]
fn order_arguments_survive_the_wire() {
    let modify = OrderRequest {
        order_id: Some("261016000123401".into()),
        trading_symbol: "M&M-EQ".into(),
        symbol_token: "2031".into(),
        price: 3388.95,
        trigger_price: 0.1 + 0.2,
        ..limit_buy()
    };
    let session = Session { client_code: "P123456".into(), jwt: "a+b/c=".into() };
    for order in [limit_buy(), modify] {
        let args = PlaceOrder { session: session.clone(), order: order.clone() };
        let decoded = over_the_wire(&args).unwrap();
        assert_eq!((&decoded.session, decoded.order), (&session, order));
    }
}

#[test]
fn quote_arguments_survive_the_wire_unless_empty() {
    let instruments = vec![
        instrument("NSE", "M&M-EQ", "2031"),
        instrument("NSE", "Nifty 50", "99926000"),
    ];
    let args = GetQuotes { session: "t".into(), instruments: instruments.clone() };
    assert_eq!(over_the_wire(&args).unwrap().instruments, instruments);

    // An empty list encodes to nothing at all, so the server cannot rebuild
    // the arguments; get_quotes documents that callers must not send one.
    let empty = GetQuotes { session: "t".into(), instruments: Vec::new() };
    assert!(over_the_wire(&empty).is_err());
}

fn mock_url() -> &'static str {
    mock();
    smartapi::TEST_BASE_URL.get().expect("set by mock()")
}

fn client_settings(client_code: &str) -> Result<ClientConfig, ServerFnError> {
    run(clients::fetch(mock_url(), mock::FIREBASE_TOKEN, mock::ENCRYPTION_KEY, client_code))
}

#[test]
fn client_settings_come_from_the_web_apps_store() {
    assert_eq!(
        client_settings("P123456").unwrap(),
        ClientConfig {
            api_key: "proxied-api-key".into(),
            proxy_url: Some("http://203.0.113.7:6001/".into()),
        }
    );
    assert_eq!(
        client_settings("D654321").unwrap(),
        ClientConfig { api_key: "direct-api-key".into(), proxy_url: None }
    );
    // Written by plug_crypto 1.x, before the web app's upgrade.
    assert_eq!(
        client_settings("L111111").unwrap(),
        ClientConfig {
            api_key: "legacy-api-key".into(),
            proxy_url: Some("http://198.51.100.20:7002".into()),
        }
    );
    let lookup = mock()
        .requests()
        .into_iter()
        .find(|r| r.path == "/clients/P123456.json")
        .unwrap();
    assert_eq!(lookup.method, Method::GET);
    assert_eq!(lookup.query.as_deref(), Some("auth=mock-firebase-token"));
}

#[test]
fn client_settings_explain_what_is_wrong() {
    assert_eq!(
        message(client_settings("NOBODY")),
        "Client NOBODY is not set up. Add it in the web app first, with its API key and proxy."
    );
    let wrong_key = run(clients::fetch(mock_url(), mock::FIREBASE_TOKEN, "another-key", "P123456"));
    assert_eq!(
        message(wrong_key),
        "Could not decrypt the settings for P123456. ENCRYPTION_KEY must match the web app's."
    );
    let wrong_token = run(clients::fetch(mock_url(), "stale", mock::ENCRYPTION_KEY, "P123456"));
    assert_eq!(
        message(wrong_token),
        "Could not load the settings for P123456 (Firebase answered HTTP 401 Unauthorized)"
    );
    // The Firebase secret must not leak into what the app shows.
    let unreachable = run(clients::fetch("http://127.0.0.1:9/", "secret-token", "k", "P123456"));
    let text = message(unreachable);
    assert!(text.starts_with("Could not load the settings for P123456: "), "{text}");
    assert!(!text.contains("secret-token"), "{text}");
}

#[test]
fn calls_leave_through_the_clients_proxy() {
    use base64::Engine;

    // The mock doubles as the proxy. The target is plain HTTP, so the proxy
    // receives the request itself instead of a CONNECT tunnel.
    let proxy = mock_url();
    let http = smartapi::build_http(Some(proxy), Some("webshare-user".into()), Some("webshare-pass".into()))
        .unwrap();
    let client = ClientConfig { api_key: "proxied-api-key".into(), proxy_url: Some(proxy.into()) };
    let request = http
        .get("http://angel.invalid/rest/secure/angelbroking/user/v1/getProfile")
        .headers(smartapi::headers(&client, Some("via-proxy")));
    // send() starts the timeout timer, so it has to run on the runtime.
    let response = run(async { request.send().await }).unwrap();
    assert!(response.status().is_success());

    let request = only(mock().requests_for("via-proxy"));
    assert_eq!(request.path, "/rest/secure/angelbroking/user/v1/getProfile");
    assert_eq!(request.header("host"), Some("angel.invalid"));
    let credentials = base64::engine::general_purpose::STANDARD.encode("webshare-user:webshare-pass");
    assert_eq!(request.header("proxy-authorization"), Some(format!("Basic {credentials}").as_str()));
    assert_eq!(request.header("x-privatekey"), Some("proxied-api-key"));
    assert_eq!(request.header("x-clientpublicip"), Some("127.0.0.1"));
}

#[test]
fn the_public_ip_header_names_the_proxy_host() {
    for (proxy, ip) in [
        ("http://203.0.113.7:6001/", "203.0.113.7"),
        ("203.0.113.7:6001", "203.0.113.7"),
        ("http://user:pass@198.51.100.20:7002", "198.51.100.20"),
    ] {
        let client = ClientConfig { api_key: "k".into(), proxy_url: Some(proxy.into()) };
        let headers = smartapi::headers(&client, None);
        assert_eq!(headers["x-clientpublicip"], ip, "{proxy}");
        assert!(!headers.contains_key("authorization"), "no JWT before sign-in");
    }
}

#[test]
fn a_proxy_that_is_not_a_url_is_refused() {
    let result = smartapi::build_http(Some("http://exa mple:80"), None, None);
    assert_eq!(message(result), "The proxy saved for this client is not a valid URL");
}

/// A proxy that answers the first request with `reply` and hands back what it
/// received. Angel One is HTTPS, so a real proxy only ever sees a CONNECT.
fn tunnel_proxy(reply: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let (sender, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        let mut buf = [0; 1024];
        while !head.ends_with(b"\r\n\r\n") {
            match socket.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => head.extend_from_slice(&buf[..n]),
            }
            // A TLS handshake never ends a line, so answer it as an HTTP
            // server would rather than wait.
            if !head[0].is_ascii_alphabetic() {
                break;
            }
        }
        let _ = socket.write_all(reply.as_bytes());
        let _ = sender.send(String::from_utf8_lossy(&head).into_owned());
    });
    (url, received)
}

const LOGIN_URL: &str = "https://angel.invalid/rest/auth/angelbroking/user/v1/loginByPassword";

#[test]
fn https_calls_open_the_tunnel_with_the_proxy_credentials() {
    use base64::Engine;

    // The stand-in closes the tunnel instead of speaking TLS, so the call
    // itself fails once the CONNECT is through.
    let (proxy, received) = tunnel_proxy("HTTP/1.1 200 Connection established\r\n\r\n");
    // Characters a URL has to escape, since reqwest carries the credentials
    // in the proxy URL until it writes the header.
    let password = "p@ss:w/rd#?";
    let http = smartapi::build_http(Some(&proxy), Some("webshare-user".into()), Some(password.into()))
        .unwrap();
    let _ = run(async { http.get(LOGIN_URL).send().await });

    let head = received.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(head.starts_with("CONNECT angel.invalid:443 HTTP/1.1\r\n"), "{head}");
    let credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("webshare-user:{password}"));
    let auth = head
        .lines()
        .find_map(|line| line.strip_prefix("Proxy-Authorization: "));
    assert_eq!(auth, Some(format!("Basic {credentials}").as_str()), "{head}");
}

#[test]
fn a_proxy_saved_as_https_is_spoken_to_in_plain_http() {
    // Webshare ports speak only plain HTTP, and trade_galleon ignores the
    // scheme, so an https:// record works in the web app. Opening TLS to the
    // port instead fails with "record overflow".
    let (proxy, received) = tunnel_proxy("HTTP/1.1 200 Connection established\r\n\r\n");
    let saved = proxy.replace("http://", "https://");
    let http = smartapi::build_http(Some(&saved), None, None).unwrap();
    let _ = run(async { http.get(LOGIN_URL).send().await });

    let head = received.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(head.starts_with("CONNECT angel.invalid:443 HTTP/1.1\r\n"), "{head:?}");
}

#[test]
fn an_unreachable_angel_one_says_why() {
    let (proxy, _) = tunnel_proxy("HTTP/1.1 407 Proxy Authentication Required\r\n\r\n");
    let http = smartapi::build_http(Some(&proxy), Some("webshare-user".into()), Some("stale".into()))
        .unwrap();
    let result = run(smartapi::send(|| http.get(LOGIN_URL), false));
    assert_eq!(
        message(result),
        "Could not reach Angel One: tunnel error: proxy authorization required"
    );
}
