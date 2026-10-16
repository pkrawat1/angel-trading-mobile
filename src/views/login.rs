use dioxus::prelude::*;

use crate::api;
use crate::auth::{use_auth, use_redirect_if_authenticated};
use crate::util;

#[component]
pub fn Login() -> Element {
    let mut client_code = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut totp = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let auth = use_auth();

    use_redirect_if_authenticated();

    let submit = move |evt: FormEvent| {
        evt.prevent_default();
        let code = client_code().trim().to_uppercase();
        let pin = password();
        let otp = totp().trim().to_string();
        if code.is_empty() || pin.is_empty() || otp.len() != 6 || !otp.chars().all(|c| c.is_ascii_digit()) {
            error.set(Some("Enter your client code, PIN and the 6-digit TOTP.".into()));
            return;
        }

        busy.set(true);
        error.set(None);
        let mut auth = auth;
        spawn(async move {
            match api::login_with_totp(code, pin, otp).await {
                // The redirect hook moves to the dashboard once the session is stored.
                Ok(tokens) => {
                    if let Err(e) = auth.login(tokens).await {
                        error.set(Some(format!("Could not save the session: {e}")));
                    }
                }
                Err(e) => error.set(Some(util::error_text(&e))),
            }
            busy.set(false);
        });
    };

    rsx! {
        div { class: "mx-auto flex max-w-sm flex-col gap-6 px-6 py-10",
            div { class: "space-y-1 text-center",
                h1 { class: "text-2xl font-bold", "Sign in to Angel One" }
                p { class: "text-sm text-base-content/60",
                    "Use your client code, PIN and the TOTP from your authenticator app."
                }
            }

            form { class: "flex flex-col gap-4", autocomplete: "off", onsubmit: submit,
                input {
                    class: "input input-bordered w-full uppercase",
                    name: "client_code",
                    placeholder: "Client code",
                    "autocapitalize": "characters",
                    "autocorrect": "off",
                    value: client_code(),
                    oninput: move |e| client_code.set(e.value()),
                }
                input {
                    class: "input input-bordered w-full",
                    r#type: "password",
                    name: "password",
                    placeholder: "PIN",
                    "inputmode": "numeric",
                    value: password(),
                    oninput: move |e| password.set(e.value()),
                }
                input {
                    class: "input input-bordered w-full tracking-widest",
                    name: "totp",
                    placeholder: "6-digit TOTP",
                    "inputmode": "numeric",
                    maxlength: "6",
                    autocomplete: "one-time-code",
                    value: totp(),
                    oninput: move |e| totp.set(e.value()),
                }
                button { class: "btn btn-primary w-full rounded-full", r#type: "submit", disabled: busy(),
                    if busy() {
                        span { class: "loading loading-spinner loading-sm" }
                        "Signing in"
                    } else {
                        "Login"
                    }
                }
            }

            if let Some(message) = error() {
                div { role: "alert", class: "alert alert-error alert-soft",
                    span { class: "text-sm", "{message}" }
                    button { class: "btn btn-ghost btn-xs", onclick: move |_| error.set(None), "✕" }
                }
            }
        }
    }
}
