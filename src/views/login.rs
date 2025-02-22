use dioxus::prelude::*;
use std::env;

#[component]
pub fn Login() -> Element {
    let mut username = use_signal(|| "".to_string());
    let mut password = use_signal(|| "".to_string());
    let mut totp = use_signal(|| "".to_string());

    rsx! {
        div {
            id: "login",
            h1 { "Login" }
            form {
                onsubmit: move |event| async move {
                    event.prevent_default();
                    let response = login_server(username(), password(), totp()).await;
                    match response {
                        Ok(message) => {
                          tracing::info!("Login successful! {message}");
                        }
                        Err(error) => {
                          tracing::error!("Login failed! {error}");
                        }
                    }
                },
                input {
                    type: "text",
                    placeholder: "Username",
                    value: "{username}" ,
                    oninput: move |event| username.set(event.value())
                }
                input {
                    type: "password",
                    placeholder: "Password",
                    value: password,
                    oninput: move |event| password.set(event.value())
                }
                input {
                  type: "totp",
                  placeholder: "TOTP",
                  value: totp,
                  oninput: move |event| totp.set(event.value())
                }
                button { "Login" }
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
    data: LoginApiResponseData,
}

#[derive(Debug, serde::Deserialize)]
struct LoginApiResponseData {
    jwtToken: String,
    refreshToken: String,
    feedToken: String,
    state: String,
}

#[server(LoginServer)]
async fn login_server(clientcode: String, password: String, totp: String) -> Result<String, ServerFnError> {
    let base_url = format!("https://apiconnect.angelbroking.com/");
    let url = format!("rest/auth/angelbroking/user/v1/loginByPassword");
    let client = reqwest::Client::new();

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert("Accept", "application/json".parse().unwrap());
    headers.insert("X-UserType", "USER".parse().unwrap());
    headers.insert("X-SourceID", "WEB".parse().unwrap());
    headers.insert("X-ClientLocalIP", env::var("LOCAL_IP").unwrap().parse().unwrap());
    headers.insert("X-ClientPublicIP", env::var("PUBLIC_IP").unwrap().parse().unwrap());
    headers.insert("X-MACAddress", env::var("MAC_ADDRESS").unwrap().parse().unwrap());
    headers.insert("X-PrivateKey", env::var("API_KEY").unwrap().parse().unwrap());

    let request = LoginApiRequest {
        clientcode,
        password,
        totp,
    };
    let response = client.post(base_url + &url)
        .headers(headers)
        .json(&request)
        .send()
        .await?;

    // log entire request
    tracing::info!("Request: {:?}", request);

    let response_text = response.text().await?;
    tracing::info!("Response Text: {:?}", response_text);

    match serde_json::from_str::<LoginApiResponse>(&response_text) {
        Ok(response_json) => {
            tracing::info!("Parsed Response JSON: {:?}", response_json);
            if response_json.status {
                Ok(format!("Login successful!"))
            } else {
                Err(ServerFnError::ServerError(response_json.message))
            }
        }
        Err(e) => {
            tracing::error!("Failed to parse JSON: {:?}", e);
            Err(ServerFnError::ServerError("Failed to parse server response".to_string()))
        }
    }
}
