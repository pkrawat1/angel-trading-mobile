use dioxus::prelude::*;

use crate::api::{self, Candle, DepthLevel, Instrument, Quote};
use crate::components::{Empty, ErrorCard, Loader, Stat};
use crate::{util, Route};

const WATCHLIST: &str = "watchlist";

#[derive(Clone, Copy, PartialEq)]
enum Range {
    Day,
    Week,
    Month,
    Year,
    FiveYears,
}

impl Range {
    const ALL: [Range; 5] = [Range::Day, Range::Week, Range::Month, Range::Year, Range::FiveYears];

    fn label(self) -> &'static str {
        match self {
            Range::Day => "1D",
            Range::Week => "1W",
            Range::Month => "1M",
            Range::Year => "1Y",
            Range::FiveYears => "5Y",
        }
    }

    fn interval(self) -> &'static str {
        match self {
            Range::Day => "FIVE_MINUTE",
            Range::Week => "FIFTEEN_MINUTE",
            Range::Month => "ONE_HOUR",
            Range::Year | Range::FiveYears => "ONE_DAY",
        }
    }

    fn days(self) -> u32 {
        match self {
            // Five days so a holiday or weekend still leaves a last session to show.
            Range::Day => 5,
            Range::Week => 7,
            Range::Month => 30,
            Range::Year => 365,
            Range::FiveYears => 1825,
        }
    }
}

fn date_of(candle: &Candle) -> &str {
    candle.time.get(..10).unwrap_or(&candle.time)
}

#[component]
pub fn QuoteDetail(exchange: String, symbol_token: String) -> Element {
    let auth = crate::auth::use_auth();
    let nav = use_navigator();

    let mut last_quote = use_signal(|| None::<Quote>);
    let quote_req = {
        let (exchange, symbol_token) = (exchange.clone(), symbol_token.clone());
        use_resource(move || {
            let (exchange, symbol_token) = (exchange.clone(), symbol_token.clone());
            async move {
                let instrument = Instrument { exchange, symbol_token, trading_symbol: String::new() };
                let result = auth.guard(api::get_quotes(auth.session(), vec![instrument]).await);
                if let Ok(quotes) = &result {
                    if let Some(q) = quotes.first() {
                        last_quote.set(Some(q.clone()));
                    }
                }
                result
            }
        })
    };
    let mut quote_req_poll = quote_req;
    use_future(move || async move {
        loop {
            util::sleep_ms(5000).await;
            quote_req_poll.restart();
        }
    });

    let mut range = use_signal(|| Range::Day);
    let candles = {
        let (exchange, symbol_token) = (exchange.clone(), symbol_token.clone());
        use_resource(move || {
            let (exchange, symbol_token) = (exchange.clone(), symbol_token.clone());
            let r = range();
            async move {
                let result = auth.guard(
                    api::get_candles(
                        auth.session(),
                        exchange,
                        symbol_token,
                        r.interval().to_string(),
                        r.days(),
                    )
                    .await,
                );
                // The previous result stays readable while a new range loads,
                // so the chart must know which range it was fetched for.
                (r, result)
            }
        })
    };

    // Tokens are unique only within an exchange, so both must match.
    let mut starred = {
        let (exchange, token) = (exchange.clone(), symbol_token.clone());
        use_signal(move || {
            storage_list()
                .iter()
                .any(|i| i.exchange == exchange && i.symbol_token == token)
        })
    };

    let toggle_star = {
        let (exchange, symbol_token) = (exchange.clone(), symbol_token.clone());
        move |_| {
            let mut list = storage_list();
            let is_this = |i: &Instrument| i.exchange == exchange && i.symbol_token == symbol_token;
            if list.iter().any(is_this) {
                list.retain(|i| !is_this(i));
                starred.set(false);
            } else {
                let trading_symbol = last_quote
                    .read()
                    .as_ref()
                    .map(|q| q.trading_symbol.clone())
                    .unwrap_or_default();
                list.push(Instrument {
                    exchange: exchange.clone(),
                    trading_symbol,
                    symbol_token: symbol_token.clone(),
                });
                starred.set(true);
            }
            let _ = crate::storage::save(WATCHLIST, &list);
        }
    };

    let star_class = if starred() { "btn btn-ghost btn-sm text-warning" } else { "btn btn-ghost btn-sm" };
    let star_icon = if starred() { "★" } else { "☆" };

    let body = match (&*last_quote.read(), &*quote_req.read()) {
        (Some(q), _) => rsx! { QuoteBody { quote: q.clone() } },
        (None, Some(Err(e))) => rsx! { ErrorCard { message: e.clone() } },
        (None, Some(Ok(_))) => rsx! { Empty { message: "No quote available for this instrument." } },
        (None, None) => rsx! { Loader {} },
    };

    let chart = match &*candles.read() {
        None => rsx! { Loader {} },
        Some((_, Err(e))) => rsx! { ErrorCard { message: e.clone() } },
        Some((fetched, Ok(all))) => {
            let shown: Vec<Candle> = if *fetched == Range::Day {
                match all.last().map(|c| date_of(c).to_string()) {
                    Some(day) => all.iter().filter(|c| date_of(c) == day).cloned().collect(),
                    None => Vec::new(),
                }
            } else {
                all.clone()
            };
            rsx! { CandleChart { candles: shown } }
        }
    };

    rsx! {
        div { class: "flex flex-col gap-4 p-4",
            div { class: "flex items-center justify-between",
                button { class: "btn btn-ghost btn-sm", onclick: move |_| { nav.go_back(); }, "‹ Back" }
                button { class: "{star_class}", onclick: toggle_star, "{star_icon}" }
            }
            {body}
            div { class: "card bg-base-100 shadow-sm",
                div { class: "card-body gap-3 p-4",
                    div { class: "join w-full",
                        for r in Range::ALL {
                            button {
                                key: "{r.label()}",
                                class: if range() == r { "btn btn-sm join-item flex-1 btn-primary" } else { "btn btn-sm join-item flex-1" },
                                onclick: move |_| range.set(r),
                                "{r.label()}"
                            }
                        }
                    }
                    {chart}
                }
            }
            div { class: "sticky bottom-2 z-10 grid grid-cols-2 gap-3",
                Link {
                    to: Route::OrderNew {
                        exchange: exchange.clone(),
                        symbol_token: symbol_token.clone(),
                        transaction_type: "BUY".to_string(),
                    },
                    class: "btn btn-success shadow-lg",
                    "Buy"
                }
                Link {
                    to: Route::OrderNew {
                        exchange: exchange.clone(),
                        symbol_token: symbol_token.clone(),
                        transaction_type: "SELL".to_string(),
                    },
                    class: "btn btn-error shadow-lg",
                    "Sell"
                }
            }
        }
    }
}

fn storage_list() -> Vec<Instrument> {
    crate::storage::load::<Vec<Instrument>>(WATCHLIST).unwrap_or_default()
}

#[component]
fn QuoteBody(quote: Quote) -> Element {
    let tone = util::tone(quote.net_change);
    let ltp = util::price(quote.ltp);
    let change = format!("{} ({})", util::signed_inr(quote.net_change), util::signed_pct(quote.percent_change));
    let symbol = util::display_symbol(&quote.trading_symbol).to_string();
    let exch = quote.exchange.clone();
    let volume = util::compact(quote.trade_volume as f64);
    let buy_qty = util::compact(quote.total_buy_qty as f64);
    let sell_qty = util::compact(quote.total_sell_qty as f64);

    rsx! {
        div { class: "card bg-base-100 shadow-sm",
            div { class: "card-body gap-3 p-4",
                div { class: "flex items-center gap-2",
                    h1 { class: "text-lg font-bold", "{symbol}" }
                    span { class: "badge badge-ghost badge-sm", "{exch}" }
                }
                div {
                    div { class: "text-4xl font-bold tabular-nums {tone}", "₹{ltp}" }
                    div { class: "text-sm tabular-nums {tone}", "{change}" }
                }
                div { class: "grid grid-cols-2 gap-3 tabular-nums",
                    Stat { label: "Open", value: util::price(quote.open) }
                    Stat { label: "Prev. close", value: util::price(quote.close) }
                    Stat { label: "High", value: util::price(quote.high) }
                    Stat { label: "Low", value: util::price(quote.low) }
                    Stat { label: "52W high", value: util::price(quote.week_52_high) }
                    Stat { label: "52W low", value: util::price(quote.week_52_low) }
                    Stat { label: "Upper circuit", value: util::price(quote.upper_circuit) }
                    Stat { label: "Lower circuit", value: util::price(quote.lower_circuit) }
                    Stat { label: "Volume", value: volume }
                    Stat { label: "Avg. price", value: util::price(quote.avg_price) }
                    Stat { label: "Total buy qty", value: buy_qty, class: "text-success" }
                    Stat { label: "Total sell qty", value: sell_qty, class: "text-error" }
                }
            }
        }
        div { class: "card bg-base-100 shadow-sm",
            div { class: "card-body gap-2 p-4",
                h2 { class: "text-sm font-semibold", "Market depth" }
                div { class: "grid grid-cols-2 gap-x-4 text-xs tabular-nums",
                    div { class: "text-base-content/60", "Bid  (qty / price)" }
                    div { class: "text-right text-base-content/60", "Ask  (price / qty)" }
                    for i in 0..5usize {
                        DepthRow {
                            key: "{i}",
                            bid: quote.depth.buy.get(i).cloned(),
                            ask: quote.depth.sell.get(i).cloned(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DepthRow(bid: Option<DepthLevel>, ask: Option<DepthLevel>) -> Element {
    let bid_qty = bid.as_ref().map(|l| l.quantity.to_string()).unwrap_or_else(|| "-".into());
    let bid_price = bid.as_ref().map(|l| util::price(l.price)).unwrap_or_else(|| "-".into());
    let ask_qty = ask.as_ref().map(|l| l.quantity.to_string()).unwrap_or_else(|| "-".into());
    let ask_price = ask.as_ref().map(|l| util::price(l.price)).unwrap_or_else(|| "-".into());
    rsx! {
        div { class: "flex justify-between border-t border-base-200 py-1 text-success",
            span { "{bid_qty}" }
            span { class: "font-semibold", "{bid_price}" }
        }
        div { class: "flex justify-between border-t border-base-200 py-1 text-error",
            span { class: "font-semibold", "{ask_price}" }
            span { "{ask_qty}" }
        }
    }
}

#[component]
fn CandleChart(candles: Vec<Candle>) -> Element {
    if candles.is_empty() {
        return rsx! { Empty { message: "No chart data for this range." } };
    }

    const W: f64 = 320.0;
    const H: f64 = 180.0;
    const PAD_X: f64 = 4.0;
    const PAD_Y: f64 = 14.0;

    let min = candles.iter().map(|c| c.low).fold(f64::MAX, f64::min);
    let max = candles.iter().map(|c| c.high).fold(f64::MIN, f64::max);
    let span = if max > min { max - min } else { 1.0 };
    let y = |v: f64| PAD_Y + (max - v) / span * (H - 2.0 * PAD_Y);
    let step = (W - 2.0 * PAD_X) / candles.len() as f64;
    let body_w = (step * 0.7).max(1.0);

    let max_label = util::price(max);
    let min_label = util::price(min);
    let min_y = H - 2.0;
    let view = format!("0 0 {W} {H}");

    rsx! {
        svg { view_box: "{view}", class: "w-full", preserve_aspect_ratio: "none",
            for (i, c) in candles.iter().enumerate() {
                {
                    let up = c.close >= c.open;
                    let cx = PAD_X + step * (i as f64 + 0.5);
                    let (y_high, y_low) = (y(c.high), y(c.low));
                    let top = y(c.open.max(c.close));
                    let height = (y(c.open.min(c.close)) - top).max(1.0);
                    let left = cx - body_w / 2.0;
                    rsx! {
                        g { key: "{i}", class: if up { "text-success" } else { "text-error" },
                            line { x1: "{cx}", x2: "{cx}", y1: "{y_high}", y2: "{y_low}", stroke: "currentColor", stroke_width: "1" }
                            rect { x: "{left}", y: "{top}", width: "{body_w}", height: "{height}", fill: "currentColor" }
                        }
                    }
                }
            }
            text { x: "{PAD_X}", y: "10", class: "fill-base-content/70", font_size: "10", "{max_label}" }
            text { x: "{PAD_X}", y: "{min_y}", class: "fill-base-content/70", font_size: "10", "{min_label}" }
        }
    }
}
