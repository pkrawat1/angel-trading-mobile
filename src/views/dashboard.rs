use dioxus::prelude::*;

use crate::api::{self, Holding, PortfolioSummary};
use crate::components::{ErrorCard, Loader, Pnl, Stat};
use crate::util;
use crate::Route;

#[component]
pub fn Dashboard() -> Element {
    let auth = crate::auth::use_auth();

    let mut profile = use_resource(move || async move { auth.guard(api::get_profile(auth.token()).await) });
    let mut funds = use_resource(move || async move { auth.guard(api::get_funds(auth.token()).await) });
    let mut holdings = use_resource(move || async move { auth.guard(api::get_holdings(auth.token()).await) });

    // Keeps the last good holdings on screen while a poll is in flight.
    let mut last_holdings = use_signal(|| None::<Vec<Holding>>);
    use_effect(move || {
        if let Some(Ok(items)) = &*holdings.read() {
            last_holdings.set(Some(items.clone()));
        }
    });

    use_future(move || async move {
        loop {
            util::sleep_ms(15_000).await;
            holdings.restart();
        }
    });

    let greeting = match &*profile.read() {
        Some(Ok(p)) => rsx! {
            div {
                h1 { class: "text-xl font-bold", "Hello, {p.name}" }
                p { class: "text-sm text-base-content/60", "Client code {p.client_code}" }
            }
        },
        Some(Err(e)) => rsx! { ErrorCard { message: e.clone() } },
        None => rsx! { div { class: "h-10" } },
    };

    let funds_card = match &*funds.read() {
        None => rsx! { Loader {} },
        Some(Err(e)) => rsx! { ErrorCard { message: e.clone() } },
        Some(Ok(f)) => {
            let available = util::inr(f.net);
            let cash = util::inr(f.available_cash);
            let margin = util::inr(f.utilised_debits);
            rsx! {
                div { class: "card bg-base-100 shadow-sm",
                    div { class: "card-body p-4 gap-3",
                        h2 { class: "text-sm font-semibold text-base-content/70", "Funds" }
                        div { class: "grid grid-cols-3 gap-2 tabular-nums",
                            Stat { label: "Available", value: available, class: "text-success" }
                            Stat { label: "Cash", value: cash }
                            Stat { label: "Margin used", value: margin }
                        }
                    }
                }
            }
        }
    };

    let current = last_holdings.read().clone();
    let holdings_section = match current {
        Some(items) => rsx! { PortfolioSection { holdings: items } },
        None => match &*holdings.read() {
            Some(Err(e)) => rsx! { ErrorCard { message: e.clone() } },
            _ => rsx! { Loader {} },
        },
    };

    rsx! {
        div { class: "flex flex-col gap-4 p-4",
            div { class: "flex items-start justify-between gap-2",
                {greeting}
                button {
                    class: "btn btn-sm btn-ghost btn-circle",
                    "aria-label": "Refresh",
                    onclick: move |_| {
                        profile.restart();
                        funds.restart();
                        holdings.restart();
                    },
                    "⟳"
                }
            }
            {funds_card}
            {holdings_section}
            div { class: "grid grid-cols-3 gap-2",
                Link { to: Route::Watchlist {}, class: "btn btn-outline btn-sm", "Watchlist" }
                Link { to: Route::Portfolio {}, class: "btn btn-outline btn-sm", "Portfolio" }
                Link { to: Route::Orders {}, class: "btn btn-outline btn-sm", "Orders" }
            }
        }
    }
}

#[component]
fn PortfolioSection(holdings: Vec<Holding>) -> Element {
    let s = PortfolioSummary::of(&holdings);
    let current = util::inr(s.current);
    let invested = util::inr(s.invested);

    let mut active: Vec<&Holding> = holdings.iter().filter(|h| h.is_active()).collect();
    active.sort_by(|a, b| b.ltp_change_pct().total_cmp(&a.ltp_change_pct()));
    let gainers: Vec<Holding> = active
        .iter()
        .filter(|h| h.ltp_change_pct() > 0.0)
        .take(3)
        .map(|h| (*h).clone())
        .collect();
    let losers: Vec<Holding> = active
        .iter()
        .rev()
        .filter(|h| h.ltp_change_pct() < 0.0)
        .take(3)
        .map(|h| (*h).clone())
        .collect();

    rsx! {
        div { class: "card bg-base-100 shadow-sm",
            div { class: "card-body p-4 gap-3",
                h2 { class: "text-sm font-semibold text-base-content/70", "Portfolio" }
                div { class: "grid grid-cols-2 gap-3 tabular-nums",
                    Stat { label: "Current value", value: current }
                    Stat { label: "Invested", value: invested }
                    div { class: "flex flex-col gap-0.5",
                        span { class: "text-xs text-base-content/60", "Overall P&L" }
                        Pnl { value: s.overall_pnl, pct: Some(s.overall_pnl_pct), class: "font-semibold" }
                    }
                    div { class: "flex flex-col gap-0.5",
                        span { class: "text-xs text-base-content/60", "Today's P&L" }
                        Pnl { value: s.todays_pnl, pct: Some(s.todays_pnl_pct), class: "font-semibold" }
                    }
                }
            }
        }
        if !gainers.is_empty() {
            MoversCard { title: "Top gainers", items: gainers }
        }
        if !losers.is_empty() {
            MoversCard { title: "Top losers", items: losers }
        }
    }
}

#[component]
fn MoversCard(title: String, items: Vec<Holding>) -> Element {
    rsx! {
        div { class: "card bg-base-100 shadow-sm",
            div { class: "card-body p-4 gap-1",
                h2 { class: "text-sm font-semibold text-base-content/70", "{title}" }
                div { class: "divide-y divide-base-200",
                    for h in items.iter() {
                        MoverRow { key: "{h.symbol_token}", holding: h.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn MoverRow(holding: Holding) -> Element {
    let ltp = util::price(holding.ltp);
    let pct = util::signed_pct(holding.ltp_change_pct());
    let tone = util::tone(holding.ltp_change_pct());
    rsx! {
        Link {
            to: Route::QuoteDetail {
                exchange: holding.exchange.clone(),
                symbol_token: holding.symbol_token.clone(),
            },
            class: "flex items-center justify-between py-2",
            span { class: "font-medium", "{holding.trading_symbol}" }
            span { class: "text-right tabular-nums",
                span { class: "mr-3", "{ltp}" }
                span { class: "{tone}", "{pct}" }
            }
        }
    }
}
