use dioxus::prelude::*;

use crate::api::{self, Instrument, Quote};
use crate::auth::{use_auth, AuthContext};
use crate::components::{Empty, ErrorCard, Loader};
use crate::{storage, util, Route};

const STORAGE_KEY: &str = "watchlist";
const EXCHANGES: [&str; 3] = ["NSE", "BSE", "NFO"];
const MIN_QUERY_CHARS: usize = 2;

fn default_watchlist() -> Vec<Instrument> {
    let index = |token: &str, symbol: &str| Instrument {
        exchange: "NSE".into(),
        trading_symbol: symbol.into(),
        symbol_token: token.into(),
    };
    vec![index("99926000", "NIFTY 50"), index("99926009", "BANKNIFTY")]
}

fn same(a: &Instrument, b: &Instrument) -> bool {
    a.exchange == b.exchange && a.symbol_token == b.symbol_token
}

fn persist(list: &[Instrument]) {
    // A failed write only loses persistence; the in-memory list stays usable.
    let _ = storage::save(STORAGE_KEY, &list.to_vec());
}

#[derive(Clone, PartialEq)]
enum SearchState {
    Idle,
    Searching,
    Failed(String),
}

// SmartAPI allows one searchScrip call per second, so each keystroke bumps a
// generation and only the last one still current after the delay hits the API.
fn schedule_search(
    auth: AuthContext,
    query: String,
    exchange: String,
    mut generation: Signal<u32>,
    mut results: Signal<Vec<Instrument>>,
    mut state: Signal<SearchState>,
) {
    let ticket = *generation.peek() + 1;
    generation.set(ticket);
    let query = query.trim().to_string();
    if query.chars().count() < MIN_QUERY_CHARS {
        results.set(Vec::new());
        state.set(SearchState::Idle);
        return;
    }
    spawn(async move {
        util::sleep_ms(1000).await;
        if *generation.peek() != ticket {
            return;
        }
        state.set(SearchState::Searching);
        let found = auth.guard(api::search_scrip(auth.token(), exchange, query).await);
        if *generation.peek() != ticket {
            return;
        }
        match found {
            Ok(items) => {
                results.set(items);
                state.set(SearchState::Idle);
            }
            Err(e) => {
                results.set(Vec::new());
                state.set(SearchState::Failed(e));
            }
        }
    });
}

#[component]
pub fn Watchlist() -> Element {
    let auth = use_auth();
    let mut list = use_signal(|| {
        storage::load::<Vec<Instrument>>(STORAGE_KEY).unwrap_or_else(default_watchlist)
    });
    let mut query = use_signal(String::new);
    let mut exchange = use_signal(|| EXCHANGES[0].to_string());
    let generation = use_signal(|| 0u32);
    let results = use_signal(Vec::<Instrument>::new);
    let state = use_signal(|| SearchState::Idle);

    let mut quotes = use_signal(Vec::<Quote>::new);
    let mut quote_error = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| false);

    let mut refresh = use_resource(move || async move {
        let items = list.read().clone();
        if items.is_empty() {
            return;
        }
        match auth.guard(api::get_quotes(auth.token(), items).await) {
            Ok(fetched) => {
                quotes.set(fetched);
                quote_error.set(None);
            }
            Err(e) => quote_error.set(Some(e)),
        }
        loaded.set(true);
    });

    use_future(move || async move {
        loop {
            util::sleep_ms(5000).await;
            refresh.restart();
        }
    });

    let toggle = move |item: Instrument| {
        let mut items = list.write();
        match items.iter().position(|i| same(i, &item)) {
            Some(pos) => {
                items.remove(pos);
            }
            None => items.push(item),
        }
        persist(&items);
    };

    let current = list.read().clone();
    let found = results.read().clone();
    let search_state = state.read().clone();
    let has_query = query.read().trim().chars().count() >= MIN_QUERY_CHARS;
    let q_snapshot = quotes.read().clone();
    let err = quote_error.read().clone();
    let is_loaded = loaded();

    rsx! {
        div { class: "flex flex-col gap-4 p-4",
            h1 { class: "text-lg font-semibold", "Watchlist" }

            div { class: "join w-full",
                select {
                    class: "select select-bordered join-item w-24",
                    value: "{exchange}",
                    onchange: move |e: FormEvent| {
                        let ex = e.value();
                        exchange.set(ex.clone());
                        schedule_search(auth, query.peek().clone(), ex, generation, results, state);
                    },
                    for ex in EXCHANGES {
                        option { value: "{ex}", selected: *exchange.read() == ex, "{ex}" }
                    }
                }
                input {
                    class: "input input-bordered join-item w-full",
                    r#type: "search",
                    placeholder: "Search for stock",
                    autocomplete: "off",
                    "autocapitalize": "characters",
                    value: "{query}",
                    oninput: move |e: FormEvent| {
                        let text = e.value();
                        query.set(text.clone());
                        schedule_search(auth, text, exchange.peek().clone(), generation, results, state);
                    },
                }
            }

            if has_query {
                div { class: "rounded-box bg-base-100 shadow-sm",
                    match search_state {
                        SearchState::Searching => rsx! {
                            div { class: "flex items-center gap-2 p-4 text-sm text-base-content/60",
                                span { class: "loading loading-spinner loading-sm" }
                                "Searching..."
                            }
                        },
                        SearchState::Failed(message) => rsx! {
                            div { class: "p-4 text-sm text-error", "{message}" }
                        },
                        SearchState::Idle => rsx! {
                            if found.is_empty() {
                                div { class: "p-4 text-sm text-base-content/60", "No results" }
                            } else {
                                ul { class: "divide-y divide-base-200",
                                    for item in found.iter() {
                                        SearchRow {
                                            key: "{item.exchange}-{item.symbol_token}",
                                            item: item.clone(),
                                            added: current.iter().any(|i| same(i, item)),
                                            on_toggle: toggle,
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }

            if current.is_empty() {
                Empty { message: "Your watchlist is empty. Search above to add stocks." }
            } else {
                if let Some(message) = err {
                    ErrorCard { message }
                }
                if !is_loaded && q_snapshot.is_empty() {
                    Loader {}
                } else {
                    ul { class: "divide-y divide-base-200 rounded-box bg-base-100 shadow-sm",
                        for item in current.iter() {
                            WatchRow {
                                key: "{item.exchange}-{item.symbol_token}",
                                item: item.clone(),
                                quote: q_snapshot.iter().find(|q| same(&q.instrument(), item)).cloned(),
                                on_remove: toggle,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchRow(item: Instrument, added: bool, on_toggle: EventHandler<Instrument>) -> Element {
    let symbol = util::display_symbol(&item.trading_symbol).to_string();
    let exchange = item.exchange.clone();
    let label = if added { "Remove from watchlist" } else { "Add to watchlist" };
    let icon = if added { "−" } else { "+" };
    let btn = if added { "btn btn-circle btn-sm btn-outline btn-error" } else { "btn btn-circle btn-sm btn-outline btn-primary" };
    rsx! {
        li { class: "flex items-center justify-between gap-2 px-4 py-2",
            div { class: "flex flex-col",
                span { class: "text-sm font-medium", "{symbol}" }
                span { class: "badge badge-ghost badge-sm", "{exchange}" }
            }
            button {
                class: "{btn}",
                "aria-label": "{label}",
                onclick: move |_| on_toggle.call(item.clone()),
                "{icon}"
            }
        }
    }
}

#[component]
fn WatchRow(item: Instrument, quote: Option<Quote>, on_remove: EventHandler<Instrument>) -> Element {
    let symbol = util::display_symbol(&item.trading_symbol).to_string();
    let exchange = item.exchange.clone();
    let to = Route::QuoteDetail {
        exchange: item.exchange.clone(),
        symbol_token: item.symbol_token.clone(),
    };
    let (ltp, change, pct, tone) = match &quote {
        Some(q) => (
            util::price(q.ltp),
            format!("{} ", util::price(q.net_change)),
            util::signed_pct(q.percent_change),
            util::tone(q.net_change),
        ),
        None => ("--".to_string(), String::new(), String::new(), "text-base-content/70"),
    };
    rsx! {
        li { class: "flex items-center",
            Link { to, class: "flex min-w-0 flex-1 items-center justify-between gap-2 py-3 pl-4",
                div { class: "flex min-w-0 flex-col",
                    span { class: "truncate text-sm font-medium", "{symbol}" }
                    span { class: "badge badge-ghost badge-sm", "{exchange}" }
                }
                div { class: "flex flex-col items-end tabular-nums",
                    span { class: "text-sm font-semibold", "{ltp}" }
                    span { class: "text-xs {tone}", "{change}{pct}" }
                }
            }
            button {
                class: "btn btn-ghost btn-sm btn-square mx-1 text-base-content/60",
                "aria-label": "Remove",
                onclick: move |_| on_remove.call(item.clone()),
                "✕"
            }
        }
    }
}
