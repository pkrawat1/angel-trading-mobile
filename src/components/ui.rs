//! Presentational pieces shared across screens.

use dioxus::prelude::*;

use crate::util;

#[component]
pub fn Loader(label: Option<String>) -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center gap-3 py-16 text-base-content/60",
            span { class: "loading loading-spinner loading-md" }
            if let Some(label) = label {
                span { class: "text-sm", "{label}" }
            }
        }
    }
}

#[component]
pub fn ErrorCard(message: String) -> Element {
    rsx! {
        div { role: "alert", class: "alert alert-error alert-soft m-4",
            span { class: "text-sm", "{message}" }
        }
    }
}

#[component]
pub fn Empty(message: String) -> Element {
    rsx! {
        div { class: "py-16 text-center text-sm text-base-content/60", "{message}" }
    }
}

/// A signed rupee amount, optionally with its percentage, coloured by direction.
#[component]
pub fn Pnl(value: f64, pct: Option<f64>, #[props(default)] class: String) -> Element {
    let tone = util::tone(value);
    let amount = util::signed_inr(value);
    let pct = pct.map(util::signed_pct);
    rsx! {
        span { class: "{tone} {class}",
            "{amount}"
            if let Some(pct) = pct {
                span { class: "ml-1 text-xs opacity-80", "({pct})" }
            }
        }
    }
}

#[component]
pub fn Stat(label: String, value: String, #[props(default)] class: String) -> Element {
    rsx! {
        div { class: "flex flex-col gap-0.5",
            span { class: "text-xs text-base-content/60", "{label}" }
            span { class: "font-semibold {class}", "{value}" }
        }
    }
}
