use std::collections::HashMap;

use dioxus::prelude::*;

use crate::api::{self, Instrument, Order, Position, Trade};
use crate::components::{Empty, ErrorCard, Loader, Pnl};
use crate::util;
use crate::Route;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Open,
    Executed,
    Positions,
    Trades,
}

fn side_badge(side: &str) -> &'static str {
    if side.eq_ignore_ascii_case("SELL") {
        "badge badge-error badge-soft badge-sm"
    } else {
        "badge badge-success badge-soft badge-sm"
    }
}

fn status_badge(status: &str) -> &'static str {
    match status.to_ascii_lowercase().as_str() {
        "complete" => "badge badge-success badge-soft badge-sm uppercase",
        "rejected" | "cancelled" => "badge badge-error badge-soft badge-sm uppercase",
        _ => "badge badge-warning badge-soft badge-sm uppercase",
    }
}

fn order_price_label(order: &Order) -> String {
    if order.price > 0.0 {
        util::inr(order.price)
    } else if order.trigger_price > 0.0 {
        format!("Trg {}", util::inr(order.trigger_price))
    } else {
        "Market".to_string()
    }
}

#[component]
pub fn Orders() -> Element {
    let auth = crate::auth::use_auth();
    let mut tab = use_signal(|| Tab::Open);
    let mut book = use_signal(|| None::<Vec<Order>>);
    let mut book_error = use_signal(|| None::<String>);

    let mut fetch = use_resource(move || async move {
        match auth.guard(api::get_order_book(auth.token()).await) {
            Ok(items) => {
                book.set(Some(items));
                book_error.set(None);
            }
            Err(e) => book_error.set(Some(e)),
        }
    });

    use_future(move || async move {
        loop {
            util::sleep_ms(10_000).await;
            fetch.restart();
        }
    });

    let current = tab();
    let tab_class = |t: Tab| if current == t { "tab tab-active" } else { "tab" };

    rsx! {
        div { class: "flex flex-col gap-4 p-4",
            h1 { class: "text-xl font-semibold", "Orders" }
            div { role: "tablist", class: "tabs tabs-box",
                button { role: "tab", class: tab_class(Tab::Open), onclick: move |_| tab.set(Tab::Open), "Open" }
                button { role: "tab", class: tab_class(Tab::Executed), onclick: move |_| tab.set(Tab::Executed), "Executed" }
                button { role: "tab", class: tab_class(Tab::Positions), onclick: move |_| tab.set(Tab::Positions), "Positions" }
                button { role: "tab", class: tab_class(Tab::Trades), onclick: move |_| tab.set(Tab::Trades), "Trades" }
            }
            match current {
                Tab::Open | Tab::Executed => rsx! {
                    match (book.read().clone(), book_error.read().clone()) {
                        (None, Some(e)) => rsx! { ErrorCard { message: e } },
                        (None, None) => rsx! { Loader {} },
                        (Some(items), err) => rsx! {
                            if let Some(e) = err {
                                ErrorCard { message: e }
                            }
                            if current == Tab::Open {
                                OpenOrders { orders: items, on_change: move |_| fetch.restart() }
                            } else {
                                ExecutedOrders { orders: items }
                            }
                        },
                    }
                },
                Tab::Positions => rsx! { PositionsTab {} },
                Tab::Trades => rsx! { TradesTab {} },
            }
        }
    }
}

#[component]
fn SymbolLink(exchange: String, symbol_token: String, symbol: String) -> Element {
    let label = util::display_symbol(&symbol).to_string();
    rsx! {
        Link {
            to: Route::QuoteDetail { exchange: exchange.clone(), symbol_token: symbol_token.clone() },
            class: "font-semibold link link-hover",
            "{label}"
        }
    }
}

#[component]
fn OpenOrders(orders: Vec<Order>, on_change: EventHandler<()>) -> Element {
    let open: Vec<Order> = orders.into_iter().filter(|o| o.is_open()).collect();
    let mut message = use_signal(|| None::<(bool, String)>);

    rsx! {
        if let Some((ok, text)) = message.read().clone() {
            div { role: "alert", class: if ok { "alert alert-success alert-soft" } else { "alert alert-error alert-soft" },
                span { class: "text-sm", "{text}" }
            }
        }
        if open.is_empty() {
            Empty { message: "No open orders" }
        } else {
            div { class: "flex flex-col gap-3",
                for o in open.iter() {
                    OpenOrderCard {
                        key: "{o.order_id}",
                        order: o.clone(),
                        on_result: move |r: (bool, String)| {
                            message.set(Some(r));
                            on_change.call(());
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn OpenOrderCard(order: Order, on_result: EventHandler<(bool, String)>) -> Element {
    let auth = crate::auth::use_auth();
    let nav = use_navigator();
    let mut confirming = use_signal(|| false);
    let mut busy = use_signal(|| false);

    let price = order_price_label(&order);
    let filled = format!("{}/{} shares", order.filled_shares, order.quantity);
    let kind = format!("{} · {}", order.order_type, order.product_type);
    let symbol_short = util::display_symbol(&order.trading_symbol).to_string();

    let variety = order.variety.clone();
    let order_id = order.order_id.clone();
    let edit_id = order.order_id.clone();

    rsx! {
        div { class: "card bg-base-100 shadow-sm",
            div { class: "card-body gap-2 p-4",
                div { class: "flex items-center justify-between",
                    SymbolLink {
                        exchange: order.exchange.clone(),
                        symbol_token: order.symbol_token.clone(),
                        symbol: order.trading_symbol.clone(),
                    }
                    span { class: "font-semibold tabular-nums", "{price}" }
                }
                div { class: "flex flex-wrap items-center gap-2 text-xs",
                    span { class: side_badge(&order.transaction_type), "{order.transaction_type}" }
                    span { class: "badge badge-ghost badge-sm", "{order.exchange}" }
                    span { class: "text-base-content/60", "{kind}" }
                }
                div { class: "flex items-center justify-between text-xs",
                    span { class: "tabular-nums text-base-content/70", "{filled}" }
                    span { class: status_badge(&order.status), "{order.status}" }
                }
                div { class: "mt-1 flex justify-end gap-2",
                    if confirming() {
                        span { class: "mr-auto self-center text-sm", "Cancel {symbol_short}?" }
                        button {
                            class: "btn btn-ghost btn-sm",
                            disabled: busy(),
                            onclick: move |_| confirming.set(false),
                            "Keep"
                        }
                        button {
                            class: "btn btn-error btn-sm",
                            disabled: busy(),
                            onclick: move |_| {
                                let variety = variety.clone();
                                let order_id = order_id.clone();
                                busy.set(true);
                                spawn(async move {
                                    let result = auth
                                        .guard(api::cancel_order(auth.token(), variety, order_id).await);
                                    busy.set(false);
                                    confirming.set(false);
                                    match result {
                                        Ok(ack) => on_result.call((true, format!("Order cancelled ({})", ack.order_id))),
                                        Err(e) => on_result.call((false, e)),
                                    }
                                });
                            },
                            if busy() {
                                span { class: "loading loading-spinner loading-xs" }
                            }
                            "Yes, cancel"
                        }
                    } else {
                        button {
                            class: "btn btn-outline btn-sm",
                            onclick: move |_| {
                                nav.push(Route::OrderEdit { order_id: edit_id.clone() });
                            },
                            "Modify"
                        }
                        button {
                            class: "btn btn-outline btn-error btn-sm",
                            onclick: move |_| confirming.set(true),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ExecutedOrders(orders: Vec<Order>) -> Element {
    let done: Vec<&Order> = orders.iter().filter(|o| !o.is_open()).collect();
    if done.is_empty() {
        return rsx! { Empty { message: "No executed orders today" } };
    }
    rsx! {
        div { class: "flex flex-col gap-3",
            for o in done {
                ExecutedCard { key: "{o.order_id}", order: o.clone() }
            }
        }
    }
}

#[component]
fn ExecutedCard(order: Order) -> Element {
    // Cancelled and rejected orders never filled, so the broker reports an average of zero. The web
    // app shows the order's own price on these cards.
    let (price_label, avg) = if order.filled_shares > 0 {
        ("Avg", util::inr(order.average_price))
    } else {
        ("Price", order_price_label(&order))
    };
    let filled = format!("{}/{} shares", order.filled_shares, order.quantity);
    let kind = format!("{} · {}", order.order_type, order.product_type);
    let time = order.update_time.clone();
    rsx! {
        div { class: "card bg-base-100 shadow-sm",
            div { class: "card-body gap-2 p-4",
                div { class: "flex items-center justify-between",
                    SymbolLink {
                        exchange: order.exchange.clone(),
                        symbol_token: order.symbol_token.clone(),
                        symbol: order.trading_symbol.clone(),
                    }
                    span { class: "text-sm tabular-nums",
                        span { class: "mr-1 text-xs text-base-content/60", "{price_label}" }
                        "{avg}"
                    }
                }
                div { class: "flex flex-wrap items-center gap-2 text-xs",
                    span { class: side_badge(&order.transaction_type), "{order.transaction_type}" }
                    span { class: "badge badge-ghost badge-sm", "{order.exchange}" }
                    span { class: "text-base-content/60", "{kind}" }
                }
                div { class: "flex items-center justify-between text-xs",
                    span { class: "tabular-nums text-base-content/70", "{filled}" }
                    span { class: status_badge(&order.status), "{order.status}" }
                }
                if !order.text.is_empty() && !order.status.eq_ignore_ascii_case("complete") {
                    p { class: "text-xs text-error", "{order.text}" }
                }
                if !time.is_empty() {
                    p { class: "text-xs text-base-content/50", "{time}" }
                }
            }
        }
    }
}

#[component]
fn PositionsTab() -> Element {
    let auth = crate::auth::use_auth();
    let data = use_resource(move || async move {
        let positions = auth.guard(api::get_positions(auth.token()).await)?;
        let missing: Vec<Instrument> = positions
            .iter()
            .filter(|p| p.ltp == 0.0)
            .map(|p| Instrument {
                exchange: p.exchange.clone(),
                trading_symbol: p.trading_symbol.clone(),
                symbol_token: p.symbol_token.clone(),
            })
            .collect();
        let mut prices = HashMap::<String, f64>::new();
        if !missing.is_empty() {
            // A failed quote call leaves LTP at zero rather than hiding the positions.
            if let Ok(quotes) = auth.guard(api::get_quotes(auth.token(), missing).await) {
                for q in quotes {
                    prices.insert(q.symbol_token, q.ltp);
                }
            }
        }
        Ok::<_, String>(
            positions
                .into_iter()
                .map(|p| {
                    let ltp = if p.ltp != 0.0 {
                        p.ltp
                    } else {
                        prices.get(&p.symbol_token).copied().unwrap_or(0.0)
                    };
                    (p, ltp)
                })
                .collect::<Vec<(Position, f64)>>(),
        )
    });

    let state = data.read().clone();
    match state {
        None => rsx! { Loader {} },
        Some(Err(e)) => rsx! { ErrorCard { message: e } },
        Some(Ok(rows)) if rows.is_empty() => rsx! { Empty { message: "No positions" } },
        Some(Ok(rows)) => {
            let total: f64 = rows.iter().map(|(p, ltp)| p.pnl(*ltp)).sum();
            rsx! {
                div { class: "flex items-center justify-between rounded-box bg-base-100 p-4 shadow-sm",
                    span { class: "text-sm text-base-content/60", "Total P&L" }
                    Pnl { value: total, class: "font-semibold tabular-nums" }
                }
                div { class: "flex flex-col gap-3",
                    for (p, ltp) in rows.iter() {
                        PositionCard { key: "{p.symbol_token}{p.product_type}", position: p.clone(), ltp: *ltp }
                    }
                }
            }
        }
    }
}

#[component]
fn PositionCard(position: Position, ltp: f64) -> Element {
    let pnl = position.pnl(ltp);
    let qty = format!("{}", position.net_qty);
    let avg = util::inr(position.avg_net_price);
    let ltp_text = if ltp == 0.0 { "-".to_string() } else { util::inr(ltp) };
    rsx! {
        div { class: "card bg-base-100 shadow-sm",
            div { class: "card-body gap-2 p-4",
                div { class: "flex items-center justify-between",
                    SymbolLink {
                        exchange: position.exchange.clone(),
                        symbol_token: position.symbol_token.clone(),
                        symbol: position.trading_symbol.clone(),
                    }
                    Pnl { value: pnl, class: "font-semibold tabular-nums" }
                }
                div { class: "flex flex-wrap items-center gap-2 text-xs",
                    span { class: "badge badge-ghost badge-sm", "{position.exchange}" }
                    span { class: "text-base-content/60", "{position.product_type}" }
                }
                div { class: "grid grid-cols-3 gap-2 text-sm tabular-nums",
                    div { class: "flex flex-col",
                        span { class: "text-xs text-base-content/60", "Net qty" }
                        span { class: "font-medium", "{qty}" }
                    }
                    div { class: "flex flex-col text-right",
                        span { class: "text-xs text-base-content/60", "Avg price" }
                        span { class: "font-medium", "{avg}" }
                    }
                    div { class: "flex flex-col text-right",
                        span { class: "text-xs text-base-content/60", "LTP" }
                        span { class: "font-medium", "{ltp_text}" }
                    }
                }
            }
        }
    }
}

#[component]
fn TradesTab() -> Element {
    let auth = crate::auth::use_auth();
    let data = use_resource(move || async move { auth.guard(api::get_trade_book(auth.token()).await) });

    let state = data.read().clone();
    match state {
        None => rsx! { Loader {} },
        Some(Err(e)) => rsx! { ErrorCard { message: e } },
        Some(Ok(trades)) if trades.is_empty() => rsx! { Empty { message: "No trades today" } },
        Some(Ok(trades)) => rsx! {
            div { class: "flex flex-col gap-3",
                for t in trades.iter() {
                    TradeCard { key: "{t.fill_id}{t.order_id}", trade: t.clone() }
                }
            }
        },
    }
}

#[component]
fn TradeCard(trade: Trade) -> Element {
    let fill = util::inr(trade.fill_price);
    let size = format!("{} shares", trade.fill_size);
    let value = util::inr(trade.trade_value);
    rsx! {
        div { class: "card bg-base-100 shadow-sm",
            div { class: "card-body gap-2 p-4",
                div { class: "flex items-center justify-between",
                    // The trade book has no symbol token, so the symbol cannot link to QuoteDetail here.
                    span { class: "font-semibold", "{util::display_symbol(&trade.trading_symbol)}" }
                    span { class: "font-semibold tabular-nums", "{fill}" }
                }
                div { class: "flex flex-wrap items-center gap-2 text-xs",
                    span { class: side_badge(&trade.transaction_type), "{trade.transaction_type}" }
                    span { class: "badge badge-ghost badge-sm", "{trade.exchange}" }
                    span { class: "text-base-content/60", "{trade.product_type}" }
                }
                div { class: "flex items-center justify-between text-xs tabular-nums text-base-content/70",
                    span { "{size}" }
                    span { "{value}" }
                }
                p { class: "text-xs text-base-content/50", "{trade.fill_time}" }
            }
        }
    }
}
