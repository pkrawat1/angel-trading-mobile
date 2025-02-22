use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn Navbar() -> Element {
    rsx! {
        div {
            id: "navbar",
            Link {
                to: Route::Login {},
                "Login"
            }
            Link {
                to: Route::Dashboard {},
                "Home"
            }
        }

        Outlet::<Route> {}
    }
}
