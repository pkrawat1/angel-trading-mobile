use dioxus::prelude::*;

use auth::{use_auth, AuthProvider, AuthState};
use components::{BottomNav, Loader, Navbar};
use views::{Dashboard, Login, OrderEdit, OrderNew, Orders, Portfolio, QuoteDetail, Watchlist};

mod api;
mod auth;
mod components;
mod storage;
mod util;
mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[layout(AppLayout)]
        #[route("/login")]
        Login {},
        #[route("/dashboard")]
        Dashboard {},
        #[route("/watchlist")]
        Watchlist {},
        #[route("/portfolio")]
        Portfolio {},
        #[route("/orders")]
        Orders {},
        #[route("/quote/:exchange/:symbol_token")]
        QuoteDetail { exchange: String, symbol_token: String },
        #[route("/order/:exchange/:symbol_token/:transaction_type")]
        OrderNew { exchange: String, symbol_token: String, transaction_type: String },
        #[route("/order/edit/:order_id")]
        OrderEdit { order_id: String },
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    configure_server_url();
    dioxus::launch(App);
}

/// `dioxus::launch` points server functions at the dx devserver on 127.0.0.1,
/// which only the iOS simulator can reach. Builds for a phone or for release
/// bake in the server's address with `APP_SERVER_URL=http://host:port`.
#[cfg(not(feature = "server"))]
fn configure_server_url() {
    if let Some(url) = option_env!("APP_SERVER_URL") {
        server_fn::client::set_server_url(url);
    }
}

#[cfg(feature = "server")]
fn configure_server_url() {}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        // viewport-fit=cover lets the layout pad itself for the notch and home indicator.
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, maximum-scale=1, viewport-fit=cover",
        }

        AuthProvider {
            Router::<Route> {}
        }
    }
}

/// Screens inside this layout render only with a session, so they can read
/// the token from `use_auth()` without guarding.
#[component]
fn AppLayout() -> Element {
    let auth = use_auth();
    let on_login = matches!(use_route::<Route>(), Route::Login {});
    let signed_in = auth.is_authenticated();

    let content = match *auth.state.read() {
        AuthState::Loading => rsx! { Loader {} },
        AuthState::Unauthenticated if !on_login => rsx! { RedirectToLogin {} },
        _ => rsx! { Outlet::<Route> {} },
    };

    rsx! {
        div { class: "flex h-dvh flex-col bg-base-200 text-base-content",
            Navbar {}
            main {
                class: "flex-1 overflow-y-auto",
                class: if signed_in { "pb-24" },
                {content}
            }
            if signed_in {
                BottomNav {}
            }
        }
    }
}

#[component]
fn RedirectToLogin() -> Element {
    let nav = use_navigator();
    use_effect(move || {
        nav.replace(Route::Login {});
    });
    rsx! { Loader {} }
}

#[component]
fn Home() -> Element {
    let auth_state = use_auth();
    let nav = use_navigator();

    use_effect(move || match *auth_state.state.read() {
        AuthState::Authenticated(_) => {
            nav.replace(Route::Dashboard {});
        }
        AuthState::Unauthenticated => {
            nav.replace(Route::Login {});
        }
        AuthState::Loading => {}
    });

    rsx! { Loader {} }
}
