use crate::auth::use_auth;
use dioxus::prelude::*;

#[component]
pub fn Navbar() -> Element {
    let auth_state = use_auth();
    let nav = use_navigator();

    let handle_logout = move |_| {
        let mut auth_state = auth_state.clone();
        let nav = nav.clone();
        spawn(async move {
            if let Err(e) = auth_state.logout().await {
                tracing::error!("Logout failed: {}", e);
            } else {
                let _ = nav.push("/login");
            }
        });
    };

    let is_authenticated = auth_state.is_authenticated();

    rsx! {
        header { class: "sticky top-0 z-10 bg-white dark:bg-gray-950 m-auto max-w-6xl px-4 sm:px-6 lg:px-8",
            div { class: "flex items-center justify-between border-b border-zinc-100 py-2 text-sm dark:border-zinc-800",
                div { class: "flex items-center gap-4 text-xl font-medium bg-clip-text bg-gradient-to-r from-red-400 to-blue-500 text-transparent",
                    a { href: "/", "Smartrade" }
                }
                div { class: "flex items-center gap-4 font-semibold leading-6 text-zinc-900",
                    if !is_authenticated {
                        a {
                            class: "px-2 py-1 bg-clip-text bg-gradient-to-r from-red-400 to-blue-500 text-transparent",
                            href: "https://www.angelone.in/open-demat-account",
                            "Open Demat Account"
                        }
                    }

                    if is_authenticated {
                        button {
                            class: "px-2 py-1 text-red-500 hover:text-red-700 cursor-pointer",
                            onclick: handle_logout,
                            "Logout"
                        }
                    }
                }
            }
        }
    }
}
