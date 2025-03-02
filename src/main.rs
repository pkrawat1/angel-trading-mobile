use dioxus::prelude::*;

use components::Navbar;
use views::{Dashboard, Login};

mod components;
mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Navbar)]
    #[route("/")]
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
    // Build cool things ✌️

    rsx! {
        // Global app resources
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        body { class: "relative h-full w-full overflow-hidden bg-white dark:bg-gray-950 antialiased",
            main { class: "relative h-full w-full flex-1 overflow-hidden transition-width mx-auto sm:py-6",
                header { class: "sticky top-0 z-10 bg-white dark:bg-gray-950 m-auto max-w-6xl px-4 sm:px-6 lg:px-8",
                    div { class: "flex items-center justify-between border-b border-zinc-100 py-2 text-sm dark:border-zinc-800",
                        div { class: "flex items-center gap-4 text-xl font-medium bg-clip-text bg-gradient-to-r from-red-400 to-blue-500 text-transparent",
                            a { href: "/", "Smartrade" }
                        }
                        div { class: "flex items-center gap-4 font-semibold leading-6 text-zinc-900",
                            a {
                                class: "px-2 py-1 bg-clip-text bg-gradient-to-r from-red-400 to-blue-500 text-transparent",
                                href: "https://www.angelone.in/open-demat-account",
                                "Open Demat Account"
                            }
                            if false {
                                a {
                                    class: "px-2 py-1 text-red-500",
                                    href: "/session/logout",
                                    "Logout"
                                }
                            }
                        }
                    }
                }
                section { class: "relative h-[calc(100%-6.5rem)] sm:h-[calc(100%-5rem)] w-full overflow-y-auto",
                    Router::<Route> {}
                }
            }
        }
    }
}
