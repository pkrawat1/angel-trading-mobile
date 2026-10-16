use dioxus::prelude::*;

use crate::{api, auth::use_auth};

#[component]
pub fn Navbar() -> Element {
    let auth = use_auth();
    let signed_in = auth.is_authenticated();

    let logout = move |_| {
        let mut auth = auth;
        spawn(async move {
            // Ending the broker session is best-effort; the local session is
            // cleared either way.
            if auth.is_authenticated() {
                let _ = api::logout_session(auth.session()).await;
            }
            if let Err(e) = auth.logout().await {
                tracing::error!("Logout failed: {e}");
            }
        });
    };

    rsx! {
        header { class: "flex items-center justify-between border-b border-base-300 bg-base-100 px-4 pb-2 pt-[calc(env(safe-area-inset-top)+0.5rem)]",
            span { class: "bg-gradient-to-r from-red-400 to-blue-500 bg-clip-text text-xl font-semibold text-transparent",
                "Smartrade"
            }
            if signed_in {
                button { class: "btn btn-ghost btn-sm text-error", onclick: logout, "Logout" }
            } else {
                a {
                    class: "text-sm font-semibold text-primary",
                    href: "https://www.angelone.in/open-demat-account",
                    "Open Demat Account"
                }
            }
        }
    }
}
