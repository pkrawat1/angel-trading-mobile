use crate::auth::{use_auth, use_redirect_if_authenticated, AuthTokens};
use crate::components::{Button, FormActions, Input, SimpleForm, ErrorMessage};
use dioxus::prelude::*;
use std::env;

#[component]
pub fn Login() -> Element {
    let mut user = use_signal(|| "".to_string());
    let mut password = use_signal(|| "".to_string());
    let mut totp = use_signal(|| "".to_string());
    let mut error_message = use_signal(|| None::<String>);
    let mut is_loading = use_signal(|| false);

    let auth = use_auth();
    let nav = use_navigator();

    // Redirect if already authenticated
    use_redirect_if_authenticated();

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

                    // Set loading state
                    is_loading.set(true);
                    error_message.set(None);

                    // All fields are valid, proceed with login
                    let mut auth = auth.clone();
                    let nav = nav.clone();
                    spawn(async move {
                        match login_server(user_val.clone(), password_val.clone(), totp_val.clone()).await {
                            Ok(tokens) => {
                                tracing::info!("Login successful, storing tokens");

                                // Store tokens using auth context
                                match auth.login(tokens).await {
                                    Ok(_) => {
                                        tracing::info!("Tokens stored successfully, redirecting to dashboard");
                                        nav.push("/dashboard");
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to store tokens: {}", e);
                                        error_message.set(Some("Failed to save session".to_string()));
                                    }
                                }
                            }
                            Err(error) => {
                                tracing::error!("Login failed: {error}");
                                error_message.set(Some("Login failed".to_string()));
                            }
                        }
                        is_loading.set(false);
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
                    required: true,
                    oninput: move |event: FormEvent| {
                        totp.set(event.data.value());
                    }
                }

                FormActions {
                    Button {
                        button_type: "submit",
                        class: "btn w-full rounded-full",
                        disabled: is_loading(),
                        if is_loading() {
                            "LOGGING IN..."
                        } else {
                            "LOGIN"
                        }
                    }
                }
            }

            // Error display using ErrorMessage component with close button
            if let Some(error) = error_message() {
                div {
                    class: "fixed top-2 right-2 w-80 sm:w-96 z-50",
                    ErrorMessage {
                        message: Some(error.clone()),
                        class: "".to_string(),
                    }
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
    state: Option<String>,
}

#[server(LoginServer)]
async fn login_server(
    clientcode: String,
    password: String,
    totp: String,
) -> Result<AuthTokens, ServerFnError> {
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
        clientcode: clientcode.clone(),
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
                if let Some(data) = response_json.data {
                    let tokens = AuthTokens {
                        jwt_token: data.jwt_token,
                        refresh_token: data.refresh_token,
                        feed_token: data.feed_token,
                        user_id: clientcode,
                    };
                    Ok(tokens)
                } else {
                    Err(ServerFnError::ServerError("No token data received".to_string()))
                }
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
