use crate::components::{Button, FormActions, Input, SimpleForm};
use dioxus::prelude::*;
use std::env;

#[component]
pub fn Login() -> Element {
    let mut user = use_signal(|| "".to_string());
    let mut password = use_signal(|| "".to_string());
    let mut totp = use_signal(|| "".to_string());
    let mut error_message = use_signal(|| None::<String>);

    rsx! {
        div { class: "flex justify-center",
            SimpleForm {
                onsubmit: move |event: FormEvent| {
                    event.prevent_default();

                    let user_val = user();
                    let password_val = password();
                    let totp_val = totp();

                    // Validate required fields
                    if user_val.is_empty() || password_val.is_empty() || totp_val.is_empty() || totp_val.len() != 6 {
                        error_message.set(Some("Invalid credentials".to_string()));
                        return;
                    }

                    // All fields are valid, proceed with login
                    spawn(async move {
                        match login_server(user_val.clone(), password_val.clone(), totp_val.clone()).await {
                            Ok(_) => {
                                // Redirect to session endpoint like Phoenix implementation
                                let redirect_url = format!("/session/{}/{}/{}", user_val, password_val, totp_val);
                                tracing::info!("Login successful, redirecting to: {}", redirect_url);
                                // For now, just log success - actual redirect would be implemented with router
                            }
                            Err(error) => {
                                tracing::error!("Login failed: {error}");
                                error_message.set(Some("Login failed".to_string()));
                            }
                        }
                    });
                },

                Input {
                    field_name: "user",
                    value: user(),
                    placeholder: "User",
                    required: true,
                    oninput: move |event: FormEvent| {
                        user.set(event.data.value());
                    }
                }

                Input {
                    field_name: "password",
                    input_type: "password",
                    value: password(),
                    placeholder: "Password",
                    maxlength: "32",
                    minlength: "8",
                    required: true,
                    oninput: move |event: FormEvent| {
                        password.set(event.data.value());
                    }
                }

                Input {
                    field_name: "totp",
                    value: totp(),
                    placeholder: "6 Digit TOTP",
                    maxlength: "6",
                    minlength: "6",
                    pattern: "[0-9]{6}",
                    required: true,
                    oninput: move |event: FormEvent| {
                        totp.set(event.data.value());
                    }
                }

                FormActions {
                    Button {
                        button_type: "submit",
                        class: "btn w-full rounded-full",
                        "LOGIN"
                    }
                }
            }

            // Error display (replicating Phoenix flash message styling)
            if let Some(error) = error_message() {
                div {
                    class: "fixed top-2 right-2 w-80 sm:w-96 z-50 rounded-lg p-3 ring-1 bg-rose-50 text-rose-900 shadow-md ring-rose-500 fill-rose-900",
                    role: "alert",
                    p {
                        class: "flex items-center gap-1.5 text-sm font-semibold leading-6",
                        span { class: "h-4 w-4", "⚠" }
                        "Error!"
                    }
                    p { class: "mt-2 text-sm leading-5", "{error}" }
                    button {
                        r#type: "button",
                        class: "group absolute top-1 right-1 p-2",
                        onclick: move |_: MouseEvent| error_message.set(None),
                        "×"
                    }
                }
            }
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct LoginApiRequest {
    clientcode: String,
    password: String,
    totp: String,
}

#[derive(Debug, serde::Deserialize)]
struct LoginApiResponse {
    status: bool,
    message: String,
    errorcode: String,
    data: Option<LoginApiResponseData>,
}

#[derive(Debug, serde::Deserialize)]
struct LoginApiResponseData {
    #[serde(rename = "jwtToken")]
    jwt_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "feedToken")]
    feed_token: String,
    state: String,
}

#[server(LoginServer)]
async fn login_server(
    clientcode: String,
    password: String,
    totp: String,
) -> Result<String, ServerFnError> {
    let base_url = "https://apiconnect.angelbroking.com/";
    let url = "rest/auth/angelbroking/user/v1/loginByPassword";
    let client = reqwest::Client::new();

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert("Accept", "application/json".parse().unwrap());
    headers.insert("X-UserType", "USER".parse().unwrap());
    headers.insert("X-SourceID", "WEB".parse().unwrap());
    headers.insert(
        "X-ClientLocalIP",
        env::var("LOCAL_IP").unwrap_or_default().parse().unwrap(),
    );
    headers.insert(
        "X-ClientPublicIP",
        env::var("PUBLIC_IP").unwrap_or_default().parse().unwrap(),
    );
    headers.insert(
        "X-MACAddress",
        env::var("MAC_ADDRESS").unwrap_or_default().parse().unwrap(),
    );
    headers.insert(
        "X-PrivateKey",
        env::var("API_KEY").unwrap_or_default().parse().unwrap(),
    );

    let request = LoginApiRequest {
        clientcode,
        password,
        totp,
    };

    let response = client
        .post(&format!("{}{}", base_url, url))
        .headers(headers)
        .json(&request)
        .send()
        .await?;

    tracing::info!("Request: {:?}", request);

    let response_text = response.text().await?;
    tracing::info!("Response Text: {:?}", response_text);

    match serde_json::from_str::<LoginApiResponse>(&response_text) {
        Ok(response_json) => {
            tracing::info!("Parsed Response JSON: {:?}", response_json);
            if response_json.status {
                Ok("Login successful!".to_string())
            } else {
                Err(ServerFnError::ServerError(response_json.message))
            }
        }
        Err(e) => {
            tracing::error!("Failed to parse JSON: {:?}", e);
            Err(ServerFnError::ServerError(
                "Failed to parse server response".to_string(),
            ))
        }
    }
}
