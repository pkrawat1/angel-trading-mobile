//! Data shapes for the SmartAPI payloads the app renders.
//!
//! Field names follow the raw SmartAPI keys, so one struct decodes the broker's
//! JSON on the server and the server-function payload on the device.

use serde::{Deserialize, Serialize};

/// SmartAPI mixes types freely: the order book sends quantities as strings,
/// RMS sends every figure as a string, and missing values arrive as `null`.
/// These decoders accept all three.
pub(crate) mod de {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    fn number(value: Option<Value>) -> f64 {
        match value {
            Some(Value::Number(n)) => n.as_f64().unwrap_or_default(),
            // Positions report amounts like "- 2235.80", with a space after the sign.
            Some(Value::String(s)) => s.replace([' ', ','], "").parse().unwrap_or_default(),
            _ => 0.0,
        }
    }

    pub fn f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        Option::<Value>::deserialize(d).map(number)
    }

    pub fn i64<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        Option::<Value>::deserialize(d).map(|v| number(v) as i64)
    }

    pub fn string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
        Ok(match Option::<Value>::deserialize(d)? {
            Some(Value::String(s)) => s,
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        })
    }

    pub fn or_default<'de, D, T>(d: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de> + Default,
    {
        Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
    }
}

pub fn percent(part: f64, whole: f64) -> f64 {
    if whole == 0.0 {
        0.0
    } else {
        part / whole * 100.0
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LoginData {
    #[serde(rename = "jwtToken", deserialize_with = "de::string")]
    pub jwt_token: String,
    #[serde(rename = "refreshToken", deserialize_with = "de::string")]
    pub refresh_token: String,
    #[serde(rename = "feedToken", deserialize_with = "de::string")]
    pub feed_token: String,
}

/// `getProfile`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    #[serde(rename = "clientcode", deserialize_with = "de::string")]
    pub client_code: String,
    #[serde(deserialize_with = "de::string")]
    pub name: String,
    #[serde(deserialize_with = "de::string")]
    pub email: String,
    #[serde(rename = "mobileno", deserialize_with = "de::string")]
    pub mobile_no: String,
    #[serde(rename = "lastlogintime", deserialize_with = "de::string")]
    pub last_login_time: String,
    #[serde(rename = "brokerid", deserialize_with = "de::string")]
    pub broker_id: String,
}

/// `getRMS`: cash and margin figures.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Funds {
    #[serde(deserialize_with = "de::f64")]
    pub net: f64,
    #[serde(rename = "availablecash", deserialize_with = "de::f64")]
    pub available_cash: f64,
    #[serde(rename = "availableintradaypayin", deserialize_with = "de::f64")]
    pub available_intraday_payin: f64,
    #[serde(rename = "availablelimitmargin", deserialize_with = "de::f64")]
    pub available_limit_margin: f64,
    #[serde(deserialize_with = "de::f64")]
    pub collateral: f64,
    #[serde(rename = "m2munrealized", deserialize_with = "de::f64")]
    pub m2m_unrealized: f64,
    #[serde(rename = "m2mrealized", deserialize_with = "de::f64")]
    pub m2m_realized: f64,
    #[serde(rename = "utiliseddebits", deserialize_with = "de::f64")]
    pub utilised_debits: f64,
    #[serde(rename = "utilisedspan", deserialize_with = "de::f64")]
    pub utilised_span: f64,
    #[serde(rename = "utilisedoptionpremium", deserialize_with = "de::f64")]
    pub utilised_option_premium: f64,
    #[serde(rename = "utilisedholdingsales", deserialize_with = "de::f64")]
    pub utilised_holding_sales: f64,
    #[serde(rename = "utilisedexposure", deserialize_with = "de::f64")]
    pub utilised_exposure: f64,
    #[serde(rename = "utilisedturnover", deserialize_with = "de::f64")]
    pub utilised_turnover: f64,
    #[serde(rename = "utilisedpayout", deserialize_with = "de::f64")]
    pub utilised_payout: f64,
}

/// One entry from `getHolding`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Holding {
    #[serde(rename = "tradingsymbol", deserialize_with = "de::string")]
    pub trading_symbol: String,
    #[serde(deserialize_with = "de::string")]
    pub exchange: String,
    #[serde(deserialize_with = "de::string")]
    pub isin: String,
    #[serde(rename = "symboltoken", deserialize_with = "de::string")]
    pub symbol_token: String,
    #[serde(deserialize_with = "de::string")]
    pub product: String,
    #[serde(deserialize_with = "de::i64")]
    pub quantity: i64,
    #[serde(rename = "t1quantity", deserialize_with = "de::i64")]
    pub t1_quantity: i64,
    #[serde(rename = "realisedquantity", deserialize_with = "de::i64")]
    pub realised_quantity: i64,
    #[serde(rename = "authorisedquantity", deserialize_with = "de::i64")]
    pub authorised_quantity: i64,
    #[serde(rename = "averageprice", deserialize_with = "de::f64")]
    pub average_price: f64,
    #[serde(deserialize_with = "de::f64")]
    pub ltp: f64,
    /// Previous session's close; today's P&L is measured against it.
    #[serde(deserialize_with = "de::f64")]
    pub close: f64,
    #[serde(rename = "profitandloss", deserialize_with = "de::f64")]
    pub profit_and_loss: f64,
    #[serde(rename = "pnlpercentage", deserialize_with = "de::f64")]
    pub pnl_percentage: f64,
}

/// Metrics computed the way the web app's `Utils.formatted_holdings` does, so
/// both apps show the same numbers.
impl Holding {
    /// False for the rows the web app hides: no token, nothing held, or no
    /// close to price against.
    pub fn is_active(&self) -> bool {
        !self.symbol_token.is_empty() && self.quantity != 0 && self.close != 0.0
    }

    /// The broker reports no average for some transferred shares; the web app
    /// prices those at the close.
    fn cost_price(&self) -> f64 {
        if self.average_price > 0.0 {
            self.average_price
        } else {
            self.close
        }
    }

    pub fn invested(&self) -> f64 {
        self.quantity as f64 * self.cost_price()
    }

    pub fn current(&self) -> f64 {
        self.quantity as f64 * self.ltp
    }

    pub fn overall_pnl(&self) -> f64 {
        self.current() - self.invested()
    }

    pub fn overall_pnl_pct(&self) -> f64 {
        percent(self.overall_pnl(), self.invested())
    }

    pub fn todays_pnl(&self) -> f64 {
        self.quantity as f64 * (self.ltp - self.close)
    }

    pub fn todays_pnl_pct(&self) -> f64 {
        percent(self.todays_pnl(), self.invested())
    }

    pub fn ltp_change_pct(&self) -> f64 {
        percent(self.ltp - self.close, self.close)
    }
}

/// Totals across active holdings, matching the web app's `Utils.calculated_overview`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PortfolioSummary {
    pub invested: f64,
    pub current: f64,
    pub overall_pnl: f64,
    pub overall_pnl_pct: f64,
    pub todays_pnl: f64,
    pub todays_pnl_pct: f64,
}

impl PortfolioSummary {
    pub fn of(holdings: &[Holding]) -> Self {
        let (invested, current, overall, today) = holdings
            .iter()
            .filter(|h| h.is_active())
            .fold((0.0, 0.0, 0.0, 0.0), |(i, c, o, t), h| {
                (i + h.invested(), c + h.current(), o + h.overall_pnl(), t + h.todays_pnl())
            });
        Self {
            invested,
            current,
            overall_pnl: overall,
            overall_pnl_pct: percent(overall, invested),
            todays_pnl: today,
            todays_pnl_pct: percent(today, invested),
        }
    }
}

/// An exchange-listed instrument: a search hit, a watchlist entry, or the key
/// for a quote request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Instrument {
    #[serde(deserialize_with = "de::string")]
    pub exchange: String,
    #[serde(rename = "tradingsymbol", deserialize_with = "de::string")]
    pub trading_symbol: String,
    #[serde(rename = "symboltoken", deserialize_with = "de::string")]
    pub symbol_token: String,
}

/// `quote` in FULL mode.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Quote {
    #[serde(deserialize_with = "de::string")]
    pub exchange: String,
    #[serde(rename = "tradingSymbol", deserialize_with = "de::string")]
    pub trading_symbol: String,
    #[serde(rename = "symbolToken", deserialize_with = "de::string")]
    pub symbol_token: String,
    #[serde(deserialize_with = "de::f64")]
    pub ltp: f64,
    #[serde(deserialize_with = "de::f64")]
    pub open: f64,
    #[serde(deserialize_with = "de::f64")]
    pub high: f64,
    #[serde(deserialize_with = "de::f64")]
    pub low: f64,
    /// Previous session's close.
    #[serde(deserialize_with = "de::f64")]
    pub close: f64,
    #[serde(rename = "netChange", deserialize_with = "de::f64")]
    pub net_change: f64,
    #[serde(rename = "percentChange", deserialize_with = "de::f64")]
    pub percent_change: f64,
    #[serde(rename = "avgPrice", deserialize_with = "de::f64")]
    pub avg_price: f64,
    #[serde(rename = "lastTradeQty", deserialize_with = "de::i64")]
    pub last_trade_qty: i64,
    #[serde(rename = "tradeVolume", deserialize_with = "de::i64")]
    pub trade_volume: i64,
    #[serde(rename = "opnInterest", deserialize_with = "de::i64")]
    pub open_interest: i64,
    #[serde(rename = "lowerCircuit", deserialize_with = "de::f64")]
    pub lower_circuit: f64,
    #[serde(rename = "upperCircuit", deserialize_with = "de::f64")]
    pub upper_circuit: f64,
    #[serde(rename = "totBuyQuan", deserialize_with = "de::i64")]
    pub total_buy_qty: i64,
    #[serde(rename = "totSellQuan", deserialize_with = "de::i64")]
    pub total_sell_qty: i64,
    #[serde(rename = "52WeekLow", deserialize_with = "de::f64")]
    pub week_52_low: f64,
    #[serde(rename = "52WeekHigh", deserialize_with = "de::f64")]
    pub week_52_high: f64,
    #[serde(rename = "exchFeedTime", deserialize_with = "de::string")]
    pub exch_feed_time: String,
    #[serde(deserialize_with = "de::or_default")]
    pub depth: Depth,
}

impl Quote {
    pub fn instrument(&self) -> Instrument {
        Instrument {
            exchange: self.exchange.clone(),
            trading_symbol: self.trading_symbol.clone(),
            symbol_token: self.symbol_token.clone(),
        }
    }
}

/// Top five bids and offers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Depth {
    #[serde(deserialize_with = "de::or_default")]
    pub buy: Vec<DepthLevel>,
    #[serde(deserialize_with = "de::or_default")]
    pub sell: Vec<DepthLevel>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DepthLevel {
    #[serde(deserialize_with = "de::f64")]
    pub price: f64,
    #[serde(deserialize_with = "de::i64")]
    pub quantity: i64,
    #[serde(deserialize_with = "de::i64")]
    pub orders: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub(crate) struct QuoteBatch {
    #[serde(deserialize_with = "de::or_default")]
    pub fetched: Vec<Quote>,
}

/// One OHLCV bar from `getCandleData`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Candle {
    /// ISO 8601 with the IST offset, e.g. `2024-01-01T09:15:00+05:30`.
    pub time: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
}

impl Candle {
    /// SmartAPI sends each bar as `[timestamp, open, high, low, close, volume]`.
    pub(crate) fn from_row(row: &[serde_json::Value]) -> Option<Self> {
        let num = |i: usize| row.get(i).and_then(|v| v.as_f64());
        Some(Self {
            time: row.first()?.as_str()?.to_string(),
            open: num(1)?,
            high: num(2)?,
            low: num(3)?,
            close: num(4)?,
            volume: num(5).unwrap_or_default() as i64,
        })
    }
}

/// One entry from `getOrderBook`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Order {
    #[serde(rename = "orderid", deserialize_with = "de::string")]
    pub order_id: String,
    #[serde(rename = "uniqueorderid", deserialize_with = "de::string")]
    pub unique_order_id: String,
    #[serde(deserialize_with = "de::string")]
    pub variety: String,
    #[serde(rename = "ordertype", deserialize_with = "de::string")]
    pub order_type: String,
    #[serde(rename = "producttype", deserialize_with = "de::string")]
    pub product_type: String,
    #[serde(deserialize_with = "de::string")]
    pub duration: String,
    #[serde(rename = "transactiontype", deserialize_with = "de::string")]
    pub transaction_type: String,
    #[serde(deserialize_with = "de::string")]
    pub exchange: String,
    #[serde(rename = "tradingsymbol", deserialize_with = "de::string")]
    pub trading_symbol: String,
    #[serde(rename = "symboltoken", deserialize_with = "de::string")]
    pub symbol_token: String,
    #[serde(rename = "instrumenttype", deserialize_with = "de::string")]
    pub instrument_type: String,
    #[serde(deserialize_with = "de::f64")]
    pub price: f64,
    #[serde(rename = "triggerprice", deserialize_with = "de::f64")]
    pub trigger_price: f64,
    #[serde(rename = "averageprice", deserialize_with = "de::f64")]
    pub average_price: f64,
    #[serde(deserialize_with = "de::i64")]
    pub quantity: i64,
    #[serde(rename = "filledshares", deserialize_with = "de::i64")]
    pub filled_shares: i64,
    #[serde(rename = "unfilledshares", deserialize_with = "de::i64")]
    pub unfilled_shares: i64,
    #[serde(rename = "lotsize", deserialize_with = "de::i64")]
    pub lot_size: i64,
    /// Lowercase broker status: "open", "complete", "rejected", "cancelled",
    /// "trigger pending", and a few transient ones.
    #[serde(deserialize_with = "de::string")]
    pub status: String,
    /// The exchange's or RMS's message, e.g. the rejection reason.
    #[serde(deserialize_with = "de::string")]
    pub text: String,
    #[serde(rename = "updatetime", deserialize_with = "de::string")]
    pub update_time: String,
    #[serde(rename = "exchtime", deserialize_with = "de::string")]
    pub exch_time: String,
}

impl Order {
    /// Still working at the exchange, so it can be modified or cancelled.
    pub fn is_open(&self) -> bool {
        !matches!(
            self.status.to_ascii_lowercase().as_str(),
            "complete" | "rejected" | "cancelled"
        )
    }
}

/// One fill from `getTradeBook`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Trade {
    #[serde(rename = "orderid", deserialize_with = "de::string")]
    pub order_id: String,
    #[serde(rename = "fillid", deserialize_with = "de::string")]
    pub fill_id: String,
    #[serde(rename = "filltime", deserialize_with = "de::string")]
    pub fill_time: String,
    #[serde(deserialize_with = "de::string")]
    pub exchange: String,
    #[serde(rename = "tradingsymbol", deserialize_with = "de::string")]
    pub trading_symbol: String,
    #[serde(rename = "producttype", deserialize_with = "de::string")]
    pub product_type: String,
    #[serde(rename = "transactiontype", deserialize_with = "de::string")]
    pub transaction_type: String,
    #[serde(rename = "fillprice", deserialize_with = "de::f64")]
    pub fill_price: f64,
    #[serde(rename = "fillsize", deserialize_with = "de::i64")]
    pub fill_size: i64,
    #[serde(rename = "tradevalue", deserialize_with = "de::f64")]
    pub trade_value: f64,
}

/// One entry from `getPosition`: today's intraday and carried-forward positions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Position {
    #[serde(deserialize_with = "de::string")]
    pub exchange: String,
    #[serde(rename = "symboltoken", deserialize_with = "de::string")]
    pub symbol_token: String,
    #[serde(rename = "tradingsymbol", deserialize_with = "de::string")]
    pub trading_symbol: String,
    #[serde(rename = "producttype", deserialize_with = "de::string")]
    pub product_type: String,
    #[serde(rename = "netqty", deserialize_with = "de::i64")]
    pub net_qty: i64,
    #[serde(rename = "buyqty", deserialize_with = "de::i64")]
    pub buy_qty: i64,
    #[serde(rename = "sellqty", deserialize_with = "de::i64")]
    pub sell_qty: i64,
    #[serde(rename = "buyavgprice", deserialize_with = "de::f64")]
    pub buy_avg_price: f64,
    #[serde(rename = "sellavgprice", deserialize_with = "de::f64")]
    pub sell_avg_price: f64,
    #[serde(rename = "avgnetprice", deserialize_with = "de::f64")]
    pub avg_net_price: f64,
    /// Includes carried-forward buys.
    #[serde(rename = "totalbuyvalue", deserialize_with = "de::f64")]
    pub total_buy_value: f64,
    /// Includes carried-forward sells.
    #[serde(rename = "totalsellvalue", deserialize_with = "de::f64")]
    pub total_sell_value: f64,
    #[serde(rename = "lotsize", deserialize_with = "de::i64")]
    pub lot_size: i64,
    /// Not always sent; fetch a quote when it is zero.
    #[serde(deserialize_with = "de::f64")]
    pub ltp: f64,
}

impl Position {
    /// Realised plus mark-to-market P&L at `ltp`.
    pub fn pnl(&self, ltp: f64) -> f64 {
        self.total_sell_value - self.total_buy_value + self.net_qty as f64 * ltp
    }
}

/// Input for placing or modifying an order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OrderRequest {
    /// Set only when modifying an existing order.
    pub order_id: Option<String>,
    /// "NORMAL", "STOPLOSS" (required for the stop-loss order types) or "AMO".
    pub variety: String,
    pub exchange: String,
    pub trading_symbol: String,
    pub symbol_token: String,
    /// "BUY" or "SELL".
    pub transaction_type: String,
    /// "MARKET", "LIMIT", "STOPLOSS_LIMIT" or "STOPLOSS_MARKET".
    pub order_type: String,
    /// "DELIVERY", "INTRADAY", "CARRYFORWARD" or "MARGIN".
    pub product_type: String,
    /// "DAY" or "IOC".
    pub duration: String,
    /// Ignored by the exchange for MARKET orders.
    pub price: f64,
    /// Used only by the stop-loss order types.
    pub trigger_price: f64,
    pub quantity: i64,
}

impl OrderRequest {
    /// SmartAPI wants every number in an order as a string.
    pub(crate) fn broker_payload(&self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "variety": self.variety,
            "exchange": self.exchange,
            "tradingsymbol": self.trading_symbol,
            "symboltoken": self.symbol_token,
            "transactiontype": self.transaction_type,
            "ordertype": self.order_type,
            "producttype": self.product_type,
            "duration": self.duration,
            "price": format!("{:.2}", self.price),
            "triggerprice": format!("{:.2}", self.trigger_price),
            "quantity": self.quantity.to_string(),
            "squareoff": "0",
            "stoploss": "0",
        });
        if let Some(order_id) = &self.order_id {
            body["orderid"] = order_id.clone().into();
        }
        body
    }
}

/// Acknowledgement from `placeOrder`, `modifyOrder` and `cancelOrder`. The
/// exchange can still reject the order afterwards; the order book has the
/// final status.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderAck {
    #[serde(rename = "orderid", deserialize_with = "de::string")]
    pub order_id: String,
    #[serde(rename = "uniqueorderid", deserialize_with = "de::string")]
    pub unique_order_id: String,
    #[serde(deserialize_with = "de::string")]
    pub script: String,
}

/// Brokerage and statutory charges for one order, from `estimateCharges`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Charges {
    pub total_charges: f64,
    pub trade_value: f64,
    pub breakup: Vec<ChargeItem>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ChargeItem {
    pub name: String,
    pub amount: f64,
}

impl Charges {
    pub(crate) fn from_summary(summary: &serde_json::Value) -> Self {
        let num = |v: &serde_json::Value| v.as_f64().unwrap_or_default();
        Self {
            total_charges: num(&summary["total_charges"]),
            trade_value: num(&summary["trade_value"]),
            breakup: summary["breakup"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| ChargeItem {
                            name: item["name"].as_str().unwrap_or_default().to_string(),
                            amount: num(&item["amount"]),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}
