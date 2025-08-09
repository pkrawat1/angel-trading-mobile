use dioxus::prelude::*;

use auth::{use_auth, AuthProvider, AuthState};
use views::{Dashboard, Login};

mod auth;
mod components;
mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Home {},
    #[layout(AppLayout)]
        #[route("/dashboard")]
        Dashboard {},
        #[route("/login")]
        Login {},
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        // Global app resources
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        AuthProvider {
            Router::<Route> {}
        }
    }
}

#[component]
fn AppLayout() -> Element {
    rsx! {
        body { class: "relative h-full w-full overflow-hidden",
            main { class: "relative h-full w-full flex-1 overflow-hidden transition-width mx-auto sm:py-6",
                AppHeader {}
                section { class: "relative h-[calc(100%-6.5rem)] sm:h-[calc(100%-5rem)] w-full overflow-y-auto",
                    Outlet::<Route> {}
                }
            }
        }
    }
}

#[component]
fn AppHeader() -> Element {
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

#[component]
fn Home() -> Element {
    let auth_state = use_auth();
    let nav = use_navigator();

    // Redirect based on auth state
    use_effect(move || {
        match *auth_state.state.read() {
            AuthState::Authenticated(_) => {
                let _ = nav.push("/dashboard");
            }
            AuthState::Unauthenticated => {
                let _ = nav.push("/login");
            }
            AuthState::Loading => {} // Wait for auth to load
        }
    });

    // Show loading while determining auth state
    rsx! {
        div { class: "flex items-center justify-center h-full",
            div { class: "text-lg font-medium text-gray-600",
                "Loading..."
            }
        }
    }
}
