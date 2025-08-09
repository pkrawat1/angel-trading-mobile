use crate::auth::use_auth;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn Navbar() -> Element {
    let auth = use_auth();
    
    rsx! {
        div {
            id: "navbar",
            class: "hidden", // Hide navbar for now since we have header in main
            if !auth.is_authenticated() {
                Link {
                    to: Route::Login {},
                    "Login"
                }
            } else {
                Link {
                    to: Route::Dashboard {},
                    "Dashboard"
                }
            }
        }

        Outlet::<Route> {}
    }
}
