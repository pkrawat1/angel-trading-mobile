use dioxus::prelude::*;

use crate::api::{self, Charges, Instrument, OrderAck, OrderRequest};
use crate::components::{ErrorCard, Loader};
use crate::{util, Route};

const PRODUCTS: [(&str, &str); 2] = [("DELIVERY", "Delivery"), ("INTRADAY", "Intraday")];
const ORDER_TYPES: [(&str, &str); 4] = [
    ("MARKET", "Market"),
    ("LIMIT", "Limit"),
    ("STOPLOSS_LIMIT", "SL"),
    ("STOPLOSS_MARKET", "SL-M"),
];

fn needs_price(order_type: &str) -> bool {
    matches!(order_type, "LIMIT" | "STOPLOSS_LIMIT")
}

fn needs_trigger(order_type: &str) -> bool {
    matches!(order_type, "STOPLOSS_LIMIT" | "STOPLOSS_MARKET")
}

fn type_label(order_type: &str) -> &'static str {
    ORDER_TYPES
        .iter()
        .find(|(value, _)| *value == order_type)
        .map(|(_, label)| *label)
        .unwrap_or("Order")
}

fn parse_f64(text: &str) -> f64 {
    text.trim().parse().unwrap_or(0.0)
}

/// Everything the form needs before it renders, so its signals start from real values.
#[derive(Clone, PartialEq)]
struct Init {
    order_id: Option<String>,
    /// Kept from the existing order when editing; derived from the order type otherwise.
    variety: Option<String>,
    instrument: Instrument,
    side: String,
    product: String,
    order_type: String,
    quantity: String,
    price: f64,
    trigger: f64,
    ltp: f64,
}

#[derive(Clone, Copy)]
struct Fields {
    base: Signal<Init>,
    side: Signal<String>,
    product: Signal<String>,
    order_type: Signal<String>,
    quantity: Signal<String>,
    price: Signal<String>,
    trigger: Signal<String>,
}

impl Fields {
    fn effective_price(&self) -> f64 {
        if needs_price(&self.order_type.read()) {
            parse_f64(&self.price.read())
        } else {
            self.base.read().ltp
        }
    }

    fn request(&self) -> OrderRequest {
        let base = self.base.read();
        let order_type = self.order_type.read().clone();
        let variety = match &base.variety {
            Some(variety) => variety.clone(),
            None if needs_trigger(&order_type) => "STOPLOSS".to_string(),
            None => "NORMAL".to_string(),
        };
        let price = if needs_price(&order_type) { parse_f64(&self.price.read()) } else { 0.0 };
        let trigger = if needs_trigger(&order_type) { parse_f64(&self.trigger.read()) } else { 0.0 };
        OrderRequest {
            order_id: base.order_id.clone(),
            variety,
            exchange: base.instrument.exchange.clone(),
            trading_symbol: base.instrument.trading_symbol.clone(),
            symbol_token: base.instrument.symbol_token.clone(),
            transaction_type: self.side.read().clone(),
            order_type,
            product_type: self.product.read().clone(),
            duration: "DAY".to_string(),
            price,
            trigger_price: trigger,
            quantity: self.quantity.read().trim().parse().unwrap_or(0),
        }
    }

    fn validate(&self) -> Result<OrderRequest, String> {
        let req = self.request();
        if req.quantity <= 0 {
            return Err("Enter a quantity greater than 0.".into());
        }
        if needs_price(&req.order_type) && req.price <= 0.0 {
            return Err("Enter a price greater than 0.".into());
        }
        if needs_trigger(&req.order_type) && req.trigger_price <= 0.0 {
            return Err("Enter a trigger price greater than 0.".into());
        }
        if req.order_type == "STOPLOSS_LIMIT" {
            if req.transaction_type == "BUY" && req.trigger_price > req.price {
                return Err("For a buy SL order the trigger must not be above the price.".into());
            }
            if req.transaction_type == "SELL" && req.trigger_price < req.price {
                return Err("For a sell SL order the trigger must not be below the price.".into());
            }
        }
        Ok(req)
    }
}

#[component]
pub fn OrderNew(exchange: String, symbol_token: String, transaction_type: String) -> Element {
    let auth = crate::auth::use_auth();
    let side = transaction_type.to_ascii_uppercase();
    let quote = use_resource(move || {
        let instrument = Instrument {
            exchange: exchange.clone(),
            trading_symbol: String::new(),
            symbol_token: symbol_token.clone(),
        };
        async move {
            let quotes = auth.guard(api::get_quotes(auth.session(), vec![instrument]).await)?;
            quotes.into_iter().next().ok_or_else(|| "No quote available for this instrument.".to_string())
        }
    });

    let state = quote.read();
    let out = match &*state {
        None => rsx! { Loader { label: "Loading quote".to_string() } },
        Some(Err(e)) => rsx! { ErrorCard { message: e.clone() } },
        Some(Ok(q)) => {
            let init = Init {
                order_id: None,
                variety: None,
                instrument: q.instrument(),
                side: if side == "SELL" { "SELL".into() } else { "BUY".into() },
                product: "DELIVERY".into(),
                order_type: "LIMIT".into(),
                quantity: String::new(),
                price: q.ltp,
                trigger: 0.0,
                ltp: q.ltp,
            };
            rsx! { OrderForm { init } }
        }
    };
    out
}

#[component]
pub fn OrderEdit(order_id: String) -> Element {
    let auth = crate::auth::use_auth();
    let loaded = use_resource(move || {
        let order_id = order_id.clone();
        async move {
            let book = auth.guard(api::get_order_book(auth.session()).await)?;
            let order = book
                .into_iter()
                .find(|o| o.order_id == order_id)
                .ok_or_else(|| "Order not found.".to_string())?;
            let instrument = Instrument {
                exchange: order.exchange.clone(),
                trading_symbol: order.trading_symbol.clone(),
                symbol_token: order.symbol_token.clone(),
            };
            // The price hint is optional here, so a failed quote falls back to the order's own price.
            let ltp = match api::get_quotes(auth.session(), vec![instrument.clone()]).await {
                Ok(quotes) => quotes.first().map(|q| q.ltp).unwrap_or(order.price),
                Err(_) => order.price,
            };
            let price = if order.price > 0.0 { order.price } else { ltp };
            Ok::<Init, String>(Init {
                order_id: Some(order.order_id.clone()),
                variety: Some(if order.variety.is_empty() { "NORMAL".into() } else { order.variety.clone() }),
                instrument,
                side: order.transaction_type.to_ascii_uppercase(),
                product: order.product_type.clone(),
                order_type: order.order_type.clone(),
                quantity: order.quantity.to_string(),
                price,
                trigger: order.trigger_price,
                ltp,
            })
        }
    });

    let state = loaded.read();
    let out = match &*state {
        None => rsx! { Loader { label: "Loading order".to_string() } },
        Some(Err(e)) => rsx! { ErrorCard { message: e.clone() } },
        Some(Ok(init)) => rsx! { OrderForm { init: init.clone() } },
    };
    out
}

#[component]
fn OrderForm(init: Init) -> Element {
    let auth = crate::auth::use_auth();
    let is_edit = init.order_id.is_some();
    // A modification keeps the order's variety, and the stop-loss types need
    // the STOPLOSS variety, so an edit offers only the types its variety takes.
    let stop_loss_variety = init.variety.as_deref() == Some("STOPLOSS");
    let symbol = util::display_symbol(&init.instrument.trading_symbol).to_string();
    let exchange = init.instrument.exchange.clone();
    let ltp = init.ltp;

    let fields = Fields {
        side: use_signal(|| init.side.clone()),
        product: use_signal(|| init.product.clone()),
        order_type: use_signal(|| init.order_type.clone()),
        quantity: use_signal(|| init.quantity.clone()),
        price: use_signal(|| format!("{:.2}", init.price)),
        trigger: use_signal(|| if init.trigger > 0.0 { format!("{:.2}", init.trigger) } else { String::new() }),
        base: use_signal(|| init),
    };
    let Fields { mut side, mut product, mut order_type, mut quantity, mut price, mut trigger, .. } = fields;

    let mut confirm = use_signal(|| None::<OrderRequest>);
    let mut problem = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut charges = use_signal(|| None::<Result<Charges, String>>);
    let mut charges_busy = use_signal(|| false);
    let mut outcome = use_signal(|| None::<OrderAck>);

    if let Some(ack) = outcome.read().as_ref() {
        let verb = if is_edit { "modified" } else { "placed" };
        let order_id = ack.order_id.clone();
        return rsx! {
            div { class: "flex flex-col gap-4 p-4",
                // DaisyUI lays an alert's children out as columns, so the
                // lines share one child to stack on a phone.
                div { role: "alert", class: "alert alert-success alert-soft",
                    div { class: "flex flex-col gap-1",
                        span { class: "font-semibold", "Order {verb}" }
                        span { class: "text-sm", "Order id {order_id}" }
                        span { class: "text-xs opacity-80",
                            "The exchange may still reject this order. Check its status in Orders."
                        }
                    }
                }
                Link { to: Route::Orders {}, class: "btn btn-primary", "View orders" }
            }
        };
    }

    let side_now = side();
    let type_now = order_type();
    let is_buy = side_now == "BUY";
    let show_price = needs_price(&type_now);
    let show_trigger = needs_trigger(&type_now);
    let ltp_text = util::price(ltp);
    let qty_now = quantity.read().trim().parse::<i64>().unwrap_or(0);
    let value = qty_now as f64 * fields.effective_price();
    let value_text = util::inr(value);
    let value_label = if show_price { "Approx. order value" } else { "Approx. value at LTP" };
    let title = format!("{} {}", if is_edit { "Modify" } else { "New" }, side_now.to_lowercase());
    let submit_class = if is_buy { "btn btn-success w-full" } else { "btn btn-error w-full" };
    let submit_label = if is_edit { format!("Review {side_now} modification") } else { format!("Review {side_now} order") };

    let on_charges = move |_| {
        let mut req = fields.request();
        if req.quantity <= 0 {
            problem.set(Some("Enter a quantity greater than 0 to estimate charges.".into()));
            return;
        }
        problem.set(None);
        if !needs_price(&req.order_type) {
            req.price = fields.base.read().ltp;
        }
        charges_busy.set(true);
        spawn(async move {
            let result = auth.guard(api::estimate_charges(auth.session(), req).await);
            charges.set(Some(result));
            charges_busy.set(false);
        });
    };

    let on_review = move |_| match fields.validate() {
        Ok(req) => {
            problem.set(None);
            confirm.set(Some(req));
        }
        Err(message) => problem.set(Some(message)),
    };

    let on_confirm = move |_| {
        // A second tap can land before the disabled state renders, and a repeat would place a second real order.
        if *busy.peek() {
            return;
        }
        let Some(req) = confirm() else { return };
        busy.set(true);
        spawn(async move {
            let session = auth.session();
            let result = if req.order_id.is_some() {
                api::modify_order(session, req).await
            } else {
                api::place_order(session, req).await
            };
            match auth.guard(result) {
                Ok(ack) => outcome.set(Some(ack)),
                Err(message) => problem.set(Some(message)),
            }
            confirm.set(None);
            busy.set(false);
        });
    };

    rsx! {
        div { class: "flex flex-col gap-4 p-4",
            div { class: "flex items-start justify-between",
                div {
                    div { class: "text-xs text-base-content/60", "{title}" }
                    div { class: "text-lg font-semibold leading-tight", "{symbol}" }
                    span { class: "badge badge-sm badge-ghost", "{exchange}" }
                }
                div { class: "text-right",
                    div { class: "text-xs text-base-content/60", "LTP" }
                    div { class: "text-lg font-semibold tabular-nums", "{ltp_text}" }
                }
            }

            div { class: "join w-full",
                for (value, label) in [("BUY", "Buy"), ("SELL", "Sell")] {
                    button {
                        key: "{value}",
                        r#type: "button",
                        disabled: is_edit || busy(),
                        class: if side() == value {
                            if value == "BUY" { "join-item btn flex-1 btn-success" } else { "join-item btn flex-1 btn-error" }
                        } else {
                            "join-item btn flex-1"
                        },
                        onclick: move |_| side.set(value.to_string()),
                        "{label}"
                    }
                }
            }

            div { class: "flex flex-col gap-1",
                span { class: "text-xs text-base-content/60", "Product" }
                div { class: "join w-full",
                    for (value, label) in PRODUCTS {
                        button {
                            key: "{value}",
                            r#type: "button",
                            // modifyOrder resends the original product type; the broker has no way to change it.
                            disabled: is_edit || busy(),
                            class: if product() == value { "join-item btn btn-sm flex-1 btn-primary" } else { "join-item btn btn-sm flex-1" },
                            onclick: move |_| product.set(value.to_string()),
                            "{label}"
                        }
                    }
                }
            }

            div { class: "flex flex-col gap-1",
                span { class: "text-xs text-base-content/60", "Order type" }
                div { class: "join w-full",
                    for (value, label) in ORDER_TYPES {
                        button {
                            key: "{value}",
                            r#type: "button",
                            disabled: busy() || (is_edit && needs_trigger(value) != stop_loss_variety),
                            class: if order_type() == value { "join-item btn btn-sm flex-1 btn-primary" } else { "join-item btn btn-sm flex-1" },
                            onclick: move |_| order_type.set(value.to_string()),
                            "{label}"
                        }
                    }
                }
            }

            label { class: "flex flex-col gap-1",
                span { class: "text-xs text-base-content/60", "Quantity" }
                input {
                    class: "input input-bordered w-full tabular-nums",
                    r#type: "number",
                    "inputmode": "numeric",
                    min: "1",
                    step: "1",
                    placeholder: "No. of shares",
                    value: "{quantity}",
                    oninput: move |e: FormEvent| quantity.set(e.value()),
                }
            }

            if show_price {
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-base-content/60", "Price" }
                    input {
                        class: "input input-bordered w-full tabular-nums",
                        r#type: "number",
                        "inputmode": "decimal",
                        step: "0.05",
                        value: "{price}",
                        oninput: move |e: FormEvent| price.set(e.value()),
                    }
                }
            }

            if show_trigger {
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-base-content/60", "Trigger price" }
                    input {
                        class: "input input-bordered w-full tabular-nums",
                        r#type: "number",
                        "inputmode": "decimal",
                        step: "0.05",
                        value: "{trigger}",
                        oninput: move |e: FormEvent| trigger.set(e.value()),
                    }
                }
            }

            div { class: "rounded-box bg-base-100 p-4 flex flex-col gap-3",
                div { class: "flex items-center justify-between",
                    span { class: "text-sm text-base-content/70", "{value_label}" }
                    span { class: "font-semibold tabular-nums", "{value_text}" }
                }
                button {
                    r#type: "button",
                    class: "btn btn-sm btn-outline self-start",
                    disabled: charges_busy() || busy(),
                    onclick: on_charges,
                    if charges_busy() {
                        span { class: "loading loading-spinner loading-xs" }
                    }
                    "Charges"
                }
                match &*charges.read() {
                    None => rsx! {},
                    Some(Err(e)) => rsx! { div { class: "text-sm text-error", "{e}" } },
                    Some(Ok(c)) => {
                        let total = util::inr(c.total_charges);
                        rsx! {
                            div { class: "flex flex-col gap-1 text-sm",
                                div { class: "flex justify-between font-semibold",
                                    span { "Total charges" }
                                    span { class: "tabular-nums", "{total}" }
                                }
                                div { class: "divide-y divide-base-200",
                                    for item in c.breakup.iter() {
                                        {
                                            let amount = util::inr(item.amount);
                                            rsx! {
                                                div { key: "{item.name}", class: "flex justify-between py-1 text-base-content/70",
                                                    span { "{item.name}" }
                                                    span { class: "tabular-nums", "{amount}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(message) = problem() {
                div { role: "alert", class: "alert alert-error alert-soft",
                    span { class: "text-sm", "{message}" }
                }
            }

            if let Some(req) = confirm() {
                {
                    let verb = if is_edit { "Modify" } else { "Place" };
                    let qty = req.quantity;
                    let sym = req.trading_symbol.clone();
                    let kind = type_label(&req.order_type);
                    let side_text = req.transaction_type.clone();
                    let prod = req.product_type.clone();
                    let price_line = if needs_price(&req.order_type) { util::inr(req.price) } else { "At market".to_string() };
                    let trigger_line = if needs_trigger(&req.order_type) { Some(util::inr(req.trigger_price)) } else { None };
                    let confirm_class = if req.transaction_type == "BUY" { "btn btn-success flex-1" } else { "btn btn-error flex-1" };
                    rsx! {
                        div { class: "card bg-base-100 shadow-sm border border-base-300",
                            div { class: "card-body gap-3 p-4",
                                h3 { class: "font-semibold", "Confirm order" }
                                div { class: "text-sm flex flex-col gap-1",
                                    div { class: "flex justify-between", span { "Side" } span { class: "font-semibold", "{side_text}" } }
                                    div { class: "flex justify-between", span { "Symbol" } span { class: "font-semibold", "{sym}" } }
                                    div { class: "flex justify-between", span { "Quantity" } span { class: "font-semibold tabular-nums", "{qty}" } }
                                    div { class: "flex justify-between", span { "Type" } span { class: "font-semibold", "{kind} / {prod}" } }
                                    div { class: "flex justify-between", span { "Price" } span { class: "font-semibold tabular-nums", "{price_line}" } }
                                    if let Some(t) = trigger_line {
                                        div { class: "flex justify-between", span { "Trigger" } span { class: "font-semibold tabular-nums", "{t}" } }
                                    }
                                }
                                p { class: "text-xs text-warning", "This sends a real order to the exchange." }
                                div { class: "flex gap-2",
                                    button {
                                        r#type: "button",
                                        class: "btn flex-1",
                                        disabled: busy(),
                                        onclick: move |_| confirm.set(None),
                                        "Cancel"
                                    }
                                    button {
                                        r#type: "button",
                                        class: "{confirm_class}",
                                        disabled: busy(),
                                        onclick: on_confirm,
                                        if busy() {
                                            span { class: "loading loading-spinner loading-xs" }
                                        }
                                        "Confirm {verb}"
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                button {
                    r#type: "button",
                    class: "{submit_class}",
                    disabled: busy(),
                    onclick: on_review,
                    "{submit_label}"
                }
            }
        }
    }
}
