use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::*;

const HOLDINGS: &str = include_str!("../../../examples/mock_smartapi/fixtures/holdings.json");
const ORDER_BOOK: &str = include_str!("../../../examples/mock_smartapi/fixtures/order_book.json");
const TRADE_BOOK: &str = include_str!("../../../examples/mock_smartapi/fixtures/trade_book.json");
const POSITIONS: &str = include_str!("../../../examples/mock_smartapi/fixtures/positions.json");
const QUOTES: &str = include_str!("../../../examples/mock_smartapi/fixtures/quotes.json");
const RMS: &str = include_str!("../../../examples/mock_smartapi/fixtures/rms.json");
const PROFILE: &str = include_str!("../../../examples/mock_smartapi/fixtures/profile.json");

#[track_caller]
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

fn parse<T: DeserializeOwned>(raw: &str) -> T {
    serde_json::from_str(raw).unwrap()
}

#[derive(Debug, Deserialize)]
struct Probe {
    #[serde(default, deserialize_with = "de::f64")]
    f: f64,
    #[serde(default, deserialize_with = "de::i64")]
    i: i64,
    #[serde(default, deserialize_with = "de::string")]
    s: String,
}

fn probe(value: Value) -> Probe {
    serde_json::from_value(value).unwrap()
}

#[test]
fn numbers_decode_from_numbers_strings_and_nulls() {
    assert_eq!(probe(json!({ "f": 12.5 })).f, 12.5);
    assert_eq!(probe(json!({ "f": "12.50" })).f, 12.5);
    assert_eq!(probe(json!({ "f": "- 2235.80" })).f, -2235.8);
    assert_eq!(probe(json!({ "f": "1,23,456.75" })).f, 123456.75);
    assert_eq!(probe(json!({ "f": null })).f, 0.0);
    assert_eq!(probe(json!({ "f": "" })).f, 0.0);
    assert_eq!(probe(json!({ "f": "n/a" })).f, 0.0);
    assert_eq!(probe(json!({})).f, 0.0);
    assert_eq!(probe(json!({ "i": "-75" })).i, -75);
    assert_eq!(probe(json!({ "i": 40 })).i, 40);
    assert_eq!(probe(json!({ "i": "10.0" })).i, 10);
    assert_eq!(probe(json!({ "s": 3045 })).s, "3045");
    assert_eq!(probe(json!({ "s": null })).s, "");
}

#[test]
fn percent_of_nothing_is_zero() {
    assert_eq!(percent(5.0, 0.0), 0.0);
    assert_eq!(percent(5.0, 200.0), 2.5);
}

#[test]
fn holding_metrics() {
    let holdings: Vec<Holding> = parse(HOLDINGS);
    let sbin = &holdings[0];
    near(sbin.invested(), 14847.0);
    near(sbin.current(), 16252.0);
    near(sbin.overall_pnl(), 1405.0);
    near(sbin.overall_pnl_pct(), 1405.0 / 14847.0 * 100.0);
    near(sbin.todays_pnl(), 150.0);
    near(sbin.todays_pnl_pct(), 150.0 / 14847.0 * 100.0);
    near(sbin.ltp_change_pct(), 7.5 / 805.1 * 100.0);

    // Transferred shares come with no average price, so they are costed at
    // the previous close, as the web app does.
    let itc = &holdings[4];
    assert_eq!(itc.average_price, 0.0);
    near(itc.invested(), 40.0 * 409.85);
    near(itc.overall_pnl(), 98.0);
}

#[test]
fn inactive_holdings() {
    let held = Holding {
        symbol_token: "3045".into(),
        quantity: 1,
        close: 800.0,
        ..Default::default()
    };
    assert!(held.is_active());
    assert!(!Holding { symbol_token: String::new(), ..held.clone() }.is_active());
    assert!(!Holding { quantity: 0, ..held.clone() }.is_active());
    assert!(!Holding { close: 0.0, ..held }.is_active());
}

#[test]
fn portfolio_summary_counts_only_active_holdings() {
    let summary = PortfolioSummary::of(&parse::<Vec<Holding>>(HOLDINGS));
    near(summary.invested, 90362.2);
    near(summary.current, 96666.9);
    near(summary.overall_pnl, 6304.7);
    near(summary.overall_pnl_pct, 6304.7 / 90362.2 * 100.0);
    near(summary.todays_pnl, 184.1);
    near(summary.todays_pnl_pct, 184.1 / 90362.2 * 100.0);
    assert_eq!(PortfolioSummary::of(&[]), PortfolioSummary::default());
}

#[test]
fn orders_stay_open_until_final() {
    let with = |status: &str| Order { status: status.into(), ..Default::default() };
    for open in ["open", "trigger pending", "open pending", "modified", "validation pending"] {
        assert!(with(open).is_open(), "{open}");
    }
    for done in ["complete", "rejected", "cancelled", "Rejected", "COMPLETE"] {
        assert!(!with(done).is_open(), "{done}");
    }
}

#[test]
fn position_pnl_counts_realised_and_open() {
    let long = Position {
        net_qty: 5,
        total_buy_value: 7376.75,
        ..Default::default()
    };
    near(long.pnl(1468.75), -33.0);
    let short = Position {
        net_qty: -75,
        total_sell_value: 12858.75,
        ..Default::default()
    };
    near(short.pnl(171.45), 0.0);
    near(short.pnl(182.35), -817.5);
}

#[test]
fn candle_rows() {
    let row = |v: Value| Candle::from_row(v.as_array().unwrap());
    assert_eq!(
        row(json!(["2026-10-16T09:15:00+05:30", 806.0, 808.5, 805.2, 807.9, 120345])),
        Some(Candle {
            time: "2026-10-16T09:15:00+05:30".into(),
            open: 806.0,
            high: 808.5,
            low: 805.2,
            close: 807.9,
            volume: 120345,
        })
    );
    assert_eq!(row(json!(["t", 1, 2, 0.5, 1.5])).map(|c| c.volume), Some(0));
    assert_eq!(row(json!(["t", 1, 2, 0.5])), None, "no close");
    assert_eq!(row(json!([1_760_000_000, 1, 2, 0.5, 1.5, 10])), None, "numeric time");
}

fn sbin_limit() -> OrderRequest {
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
        price: 798.0,
        trigger_price: 0.0,
        quantity: 10,
    }
}

#[test]
fn order_payload_sends_strings_and_omits_a_missing_id() {
    let payload = sbin_limit().broker_payload();
    assert_eq!(
        payload,
        json!({
            "variety": "NORMAL",
            "exchange": "NSE",
            "tradingsymbol": "SBIN-EQ",
            "symboltoken": "3045",
            "transactiontype": "BUY",
            "ordertype": "LIMIT",
            "producttype": "DELIVERY",
            "duration": "DAY",
            "price": "798.00",
            "triggerprice": "0.00",
            "quantity": "10",
            "squareoff": "0",
            "stoploss": "0"
        })
    );

    let modify = OrderRequest {
        order_id: Some("261016000123401".into()),
        price: 798.456,
        trigger_price: 797.5,
        ..sbin_limit()
    };
    let payload = modify.broker_payload();
    assert_eq!(payload["orderid"], "261016000123401");
    assert_eq!(payload["price"], "798.46");
    assert_eq!(payload["triggerprice"], "797.50");
}

#[test]
fn charges_from_the_summary() {
    let summary = json!({
        "total_charges": 12.3456,
        "trade_value": 8126,
        "breakup": [
            { "name": "Angel One Brokerage", "amount": 8.126, "msg": "", "breakup": [] },
            { "name": "Taxes", "amount": 1.5 }
        ]
    });
    assert_eq!(
        Charges::from_summary(&summary),
        Charges {
            total_charges: 12.3456,
            trade_value: 8126.0,
            breakup: vec![
                ChargeItem { name: "Angel One Brokerage".into(), amount: 8.126 },
                ChargeItem { name: "Taxes".into(), amount: 1.5 },
            ],
        }
    );
    assert_eq!(Charges::from_summary(&Value::Null), Charges::default());
}

/// The server serialises each model and the device reads it back with the
/// same struct, so the renamed keys and lenient decoders must agree.
#[track_caller]
fn survives_the_trip_to_the_device<T>(value: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let json = serde_json::to_string(value).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&json).unwrap(), value, "{json}");
}

#[test]
fn models_survive_the_trip_to_the_device() {
    survives_the_trip_to_the_device(&parse::<Profile>(PROFILE));
    survives_the_trip_to_the_device(&parse::<Funds>(RMS));
    survives_the_trip_to_the_device(&parse::<Vec<Holding>>(HOLDINGS));
    survives_the_trip_to_the_device(&parse::<Vec<Order>>(ORDER_BOOK));
    survives_the_trip_to_the_device(&parse::<Vec<Trade>>(TRADE_BOOK));
    survives_the_trip_to_the_device(&parse::<Vec<Position>>(POSITIONS));
    survives_the_trip_to_the_device(&parse::<Vec<Quote>>(QUOTES));
    survives_the_trip_to_the_device(&sbin_limit());
    survives_the_trip_to_the_device(&Candle {
        time: "2026-10-16T09:15:00+05:30".into(),
        open: 806.0,
        high: 808.5,
        low: 805.2,
        close: 807.9,
        volume: 120345,
    });
    survives_the_trip_to_the_device(&OrderAck {
        order_id: "261016000123401".into(),
        unique_order_id: "6b1d2c3e-mock-4f00-9a01-000000123401".into(),
        script: "SBIN-EQ".into(),
    });
    survives_the_trip_to_the_device(&Charges {
        total_charges: 3.08,
        trade_value: 2400.0,
        breakup: vec![ChargeItem { name: "Taxes".into(), amount: 0.0136 }],
    });
}
