use crate::auth::use_require_auth;
use crate::components::{Echo, Hero};
use dioxus::prelude::*;

#[component]
pub fn Dashboard() -> Element {
    // Require authentication to access dashboard
    let is_authenticated = use_require_auth();
    
    if !is_authenticated {
        return rsx! {
            div { class: "flex items-center justify-center h-full",
                div { class: "text-lg font-medium text-gray-600",
                    "Redirecting to login..."
                }
            }
        };
    }

    rsx! {
        div { class: "p-4",
            div { class: "mb-4",
                h1 { class: "text-2xl font-bold text-gray-900", "Dashboard" }
                p { class: "text-gray-600", "Welcome to your trading dashboard" }
            }
            Hero {}
            Echo {}
        }
    }
}
