use crate::components::{Echo, Hero};
use dioxus::prelude::*;

#[component]
pub fn Dashboard() -> Element {
    rsx! {
        Hero {}
        Echo {}
    }
}
