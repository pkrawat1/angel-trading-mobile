use dioxus::prelude::*;

use crate::api::{self, Holding, PortfolioSummary};
use crate::components::{Empty, ErrorCard, Loader, Pnl, Stat};
use crate::{util, Route};

#[derive(Clone, Copy, PartialEq)]
enum SortBy {
    Symbol,
    Value,
    Pnl,
    Today,
}

impl SortBy {
    const ALL: [SortBy; 4] = [SortBy::Symbol, SortBy::Value, SortBy::Pnl, SortBy::Today];

    fn label(self) -> &'static str {
        match self {
            SortBy::Symbol => "Symbol",
            SortBy::Value => "Value",
            SortBy::Pnl => "P&L %",
            SortBy::Today => "Today %",
        }
    }

    fn apply(self, items: &mut [Holding]) {
        match self {
            SortBy::Symbol => items.sort_by(|a, b| a.trading_symbol.cmp(&b.trading_symbol)),
            SortBy::Value => items.sort_by(|a, b| b.current().total_cmp(&a.current())),
            SortBy::Pnl => items.sort_by(|a, b| b.overall_pnl_pct().total_cmp(&a.overall_pnl_pct())),
            SortBy::Today => items.sort_by(|a, b| b.ltp_change_pct().total_cmp(&a.ltp_change_pct())),
        }
    }
}

#[component]
pub fn Portfolio() -> Element {
    let auth = crate::auth::use_auth();
    let mut data = use_resource(move || async move { auth.guard(api::get_holdings(auth.session()).await) });
    let mut holdings = use_signal(|| None::<Vec<Holding>>);
    let mut error = use_signal(|| None::<String>);
    let mut sort = use_signal(|| SortBy::Symbol);

    use_effect(move || match &*data.read() {
        Some(Ok(items)) => {
            holdings.set(Some(items.clone()));
            error.set(None);
        }
        Some(Err(e)) => error.set(Some(e.clone())),
        None => {}
    });

    use_future(move || async move {
        loop {
            util::sleep_ms(15000).await;
            data.restart();
        }
    });

    let Some(all) = holdings.read().clone() else {
        if let Some(e) = error.read().clone() {
            return rsx! { ErrorCard { message: e } };
        }
        return rsx! { Loader { label: "Loading holdings".to_string() } };
    };

    let mut active: Vec<Holding> = all.into_iter().filter(|h| h.is_active()).collect();
    if active.is_empty() {
        return rsx! { Empty { message: "No holdings yet".to_string() } };
    }
    let summary = PortfolioSummary::of(&active);
    sort().apply(&mut active);

    let current = util::inr(summary.current);
    let invested = util::inr(summary.invested);
    let count = active.len();

    rsx! {
        div { class: "flex flex-col gap-4 p-4",
            if let Some(e) = error.read().clone() {
                div { role: "alert", class: "alert alert-warning alert-soft text-sm", "{e}" }
            }
            div { class: "card bg-base-100 shadow-sm",
                div { class: "card-body gap-3 p-4",
                    div { class: "flex flex-col gap-0.5",
                        span { class: "text-xs text-base-content/60", "Current value" }
                        span { class: "text-2xl font-bold tabular-nums", "{current}" }
                    }
                    div { class: "grid grid-cols-2 gap-3",
                        Stat { label: "Invested".to_string(), value: invested, class: "tabular-nums".to_string() }
                        div { class: "flex flex-col gap-0.5",
                            span { class: "text-xs text-base-content/60", "Overall P&L" }
                            Pnl { value: summary.overall_pnl, pct: Some(summary.overall_pnl_pct), class: "font-semibold tabular-nums".to_string() }
                        }
                        div { class: "flex flex-col gap-0.5",
                            span { class: "text-xs text-base-content/60", "Today's P&L" }
                            Pnl { value: summary.todays_pnl, pct: Some(summary.todays_pnl_pct), class: "font-semibold tabular-nums".to_string() }
                        }
                    }
                }
            }
            div { class: "flex items-center justify-between",
                span { class: "text-sm font-semibold", "Holdings ({count})" }
                div { class: "join",
                    for s in SortBy::ALL {
                        button {
                            key: "{s.label()}",
                            class: if sort() == s { "btn btn-xs join-item btn-primary" } else { "btn btn-xs join-item" },
                            onclick: move |_| sort.set(s),
                            "{s.label()}"
                        }
                    }
                }
            }
            div { class: "card bg-base-100 shadow-sm",
                div { class: "divide-y divide-base-200",
                    for h in active.iter() {
                        HoldingRow { key: "{h.exchange}-{h.symbol_token}", holding: h.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn HoldingRow(holding: Holding) -> Element {
    let qty = holding.quantity;
    let avg = util::price(holding.average_price);
    let ltp = util::price(holding.ltp);
    let change = util::signed_pct(holding.ltp_change_pct());
    let change_tone = util::tone(holding.ltp_change_pct());
    let symbol = util::display_symbol(&holding.trading_symbol).to_string();

    rsx! {
        Link {
            to: Route::QuoteDetail { exchange: holding.exchange.clone(), symbol_token: holding.symbol_token.clone() },
            class: "flex items-start justify-between gap-3 p-3 active:bg-base-200",
            div { class: "flex min-w-0 flex-col gap-0.5",
                span { class: "truncate font-semibold", "{symbol}" }
                span { class: "text-xs text-base-content/60 tabular-nums", "{qty} @ {avg}" }
            }
            div { class: "flex shrink-0 flex-col items-end gap-0.5 text-sm tabular-nums",
                span {
                    "{ltp} "
                    span { class: "text-xs {change_tone}", "{change}" }
                }
                Pnl { value: holding.overall_pnl(), pct: Some(holding.overall_pnl_pct()), class: "text-xs".to_string() }
            }
        }
    }
}
