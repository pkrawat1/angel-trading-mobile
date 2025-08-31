use dioxus::prelude::*;

use auth::{use_auth, AuthProvider, AuthState};
use views::{Dashboard, Login};
use components::Navbar;

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
                Navbar {}
                section { class: "relative h-[calc(100%-6.5rem)] sm:h-[calc(100%-5rem)] w-full overflow-y-auto",
                    Outlet::<Route> {}
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
