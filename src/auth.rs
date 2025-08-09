use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use web_sys::{window, Storage};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthTokens {
    pub jwt_token: String,
    pub refresh_token: String,
    pub feed_token: String,
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    Loading,
    Authenticated(AuthTokens),
    Unauthenticated,
}

const TOKEN_STORAGE_KEY: &str = "angel_trading_auth_tokens";
const TOKEN_EXPIRY_KEY: &str = "angel_trading_auth_expiry";

// Context for auth state
#[derive(Clone, Copy)]
pub struct AuthContext {
    pub state: Signal<AuthState>,
}

impl AuthContext {
    pub fn new() -> Self {
        Self {
            state: Signal::new(AuthState::Loading),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        matches!(*self.state.read(), AuthState::Authenticated(_))
    }

    pub fn is_loading(&self) -> bool {
        matches!(*self.state.read(), AuthState::Loading)
    }

    pub fn get_tokens(&self) -> Option<AuthTokens> {
        match &*self.state.read() {
            AuthState::Authenticated(tokens) => Some(tokens.clone()),
            _ => None,
        }
    }

    pub async fn login(&mut self, tokens: AuthTokens) -> Result<(), String> {
        // Store tokens in localStorage
        store_auth_tokens(&tokens).await?;

        // Update auth state
        self.state.set(AuthState::Authenticated(tokens));
        Ok(())
    }

    pub async fn logout(&mut self) -> Result<(), String> {
        // Clear tokens from storage
        clear_auth_tokens().await?;

        // Update auth state
        self.state.set(AuthState::Unauthenticated);
        Ok(())
    }

    pub fn get_auth_header(&self) -> Option<(String, String)> {
        self.get_tokens()
            .map(|tokens| ("Authorization".to_string(), format!("Bearer {}", tokens.jwt_token)))
    }

    pub async fn refresh_tokens_if_needed(&mut self) -> Result<(), String> {
        if let Some(tokens) = self.get_tokens() {
            // In a real app, you'd check token expiry and call refresh endpoint
            // For now, we'll just validate that tokens exist
            if self.is_token_expired(&tokens).await {
                // Clear expired tokens
                self.logout().await?;
                return Err("Tokens expired".to_string());
            }
        }
        Ok(())
    }

    async fn is_token_expired(&self, _tokens: &AuthTokens) -> bool {
        // Check if tokens are expired based on storage expiry
        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(storage) = get_local_storage() {
                if let Ok(Some(expiry_str)) = storage.get_item(TOKEN_EXPIRY_KEY) {
                    if let Ok(expiry) = expiry_str.parse::<f64>() {
                        return js_sys::Date::now() > expiry;
                    }
                }
            }
        }
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(app_dir) = get_app_data_dir() {
                let expiry_path = app_dir.join("auth_expiry.txt");
                if let Ok(expiry_str) = std::fs::read_to_string(&expiry_path) {
                    if let Ok(expiry) = expiry_str.trim().parse::<u64>() {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        return now > expiry;
                    }
                }
            }
        }
        
        false
    }

    pub fn validate_and_get_tokens(&self) -> Option<AuthTokens> {
        match &*self.state.read() {
            AuthState::Authenticated(tokens) => {
                // Basic validation - check if required fields are present
                if !tokens.jwt_token.is_empty() && 
                   !tokens.refresh_token.is_empty() && 
                   !tokens.feed_token.is_empty() && 
                   !tokens.user_id.is_empty() {
                    Some(tokens.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

// Hook to get auth context
pub fn use_auth() -> AuthContext {
    use_context::<AuthContext>()
}

// Hook for protecting routes that require authentication
pub fn use_require_auth() -> bool {
    let auth = use_auth();
    let nav = use_navigator();

    use_effect(move || {
        if !auth.is_loading() && !auth.is_authenticated() {
            nav.push("/login");
        }
    });

    auth.is_authenticated()
}

// Hook for redirecting authenticated users away from login
pub fn use_redirect_if_authenticated() {
    let auth = use_auth();
    let nav = use_navigator();

    use_effect(move || {
        if auth.is_authenticated() {
            nav.push("/dashboard");
        }
    });
}

// Storage utilities with better error handling and expiry support
async fn store_auth_tokens(tokens: &AuthTokens) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = get_local_storage()?;

        let tokens_json = serde_json::to_string(tokens)
            .map_err(|e| format!("Failed to serialize tokens: {}", e))?;

        // Store tokens
        storage
            .set_item(TOKEN_STORAGE_KEY, &tokens_json)
            .map_err(|e| format!("Failed to store tokens: {:?}", e))?;

        // Store expiry timestamp (24 hours from now)
        let expiry = js_sys::Date::now() + (24.0 * 60.0 * 60.0 * 1000.0);
        storage
            .set_item(TOKEN_EXPIRY_KEY, &expiry.to_string())
            .map_err(|e| format!("Failed to store token expiry: {:?}", e))?;

        tracing::info!("Tokens stored successfully with expiry");
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // For mobile platforms, use file-based storage
        store_tokens_file(tokens).await?;
    }

    Ok(())
}

async fn load_auth_from_storage() -> AuthState {
    #[cfg(target_arch = "wasm32")]
    {
        match get_local_storage() {
            Ok(storage) => {
                // Check if tokens have expired
                if let Ok(Some(expiry_str)) = storage.get_item(TOKEN_EXPIRY_KEY) {
                    if let Ok(expiry) = expiry_str.parse::<f64>() {
                        let now = js_sys::Date::now();
                        if now > expiry {
                            tracing::info!("Tokens expired, clearing storage");
                            let _ = clear_auth_tokens_internal(&storage);
                            return AuthState::Unauthenticated;
                        }
                    }
                }

                // Load tokens if not expired
                if let Ok(Some(tokens_json)) = storage.get_item(TOKEN_STORAGE_KEY) {
                    match serde_json::from_str::<AuthTokens>(&tokens_json) {
                        Ok(tokens) => {
                            tracing::info!("Loaded valid tokens from storage");
                            return AuthState::Authenticated(tokens);
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse stored tokens: {}", e);
                            let _ = clear_auth_tokens_internal(&storage);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to access storage: {}", e);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // For mobile platforms, try loading from file
        match load_tokens_file().await {
            Ok(tokens) => return AuthState::Authenticated(tokens),
            Err(e) => tracing::info!("No valid tokens found in file storage: {}", e),
        }
    }

    AuthState::Unauthenticated
}

async fn clear_auth_tokens() -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = get_local_storage()?;
        clear_auth_tokens_internal(&storage)?;
        tracing::info!("Tokens cleared from storage");
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        clear_tokens_file().await?;
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn get_local_storage() -> Result<Storage, String> {
    let window = window().ok_or("No window available")?;
    window
        .local_storage()
        .map_err(|e| format!("Failed to access localStorage: {:?}", e))?
        .ok_or("localStorage not available".to_string())
}

#[cfg(target_arch = "wasm32")]
fn clear_auth_tokens_internal(storage: &Storage) -> Result<(), String> {
    storage
        .remove_item(TOKEN_STORAGE_KEY)
        .map_err(|e| format!("Failed to clear tokens: {:?}", e))?;

    storage
        .remove_item(TOKEN_EXPIRY_KEY)
        .map_err(|e| format!("Failed to clear token expiry: {:?}", e))?;

    Ok(())
}

// File-based storage for mobile/desktop platforms
#[cfg(not(target_arch = "wasm32"))]
async fn store_tokens_file(tokens: &AuthTokens) -> Result<(), String> {
    use std::fs;


    let app_dir = get_app_data_dir()?;
    fs::create_dir_all(&app_dir).map_err(|e| format!("Failed to create app directory: {}", e))?;
    
    let tokens_path = app_dir.join("auth_tokens.json");
    let expiry_path = app_dir.join("auth_expiry.txt");
    
    // Store tokens
    let tokens_json = serde_json::to_string(tokens)
        .map_err(|e| format!("Failed to serialize tokens: {}", e))?;
    fs::write(&tokens_path, tokens_json)
        .map_err(|e| format!("Failed to write tokens file: {}", e))?;
    
    // Store expiry (24 hours from now)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expiry = now + (24 * 60 * 60); // 24 hours
    fs::write(&expiry_path, expiry.to_string())
        .map_err(|e| format!("Failed to write expiry file: {}", e))?;
    
    tracing::info!("Tokens stored to file successfully");
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn load_tokens_file() -> Result<AuthTokens, String> {
    use std::fs;
    
    let app_dir = get_app_data_dir()?;
    let tokens_path = app_dir.join("auth_tokens.json");
    let expiry_path = app_dir.join("auth_expiry.txt");
    
    // Check expiry first
    if expiry_path.exists() {
        let expiry_str = fs::read_to_string(&expiry_path)
            .map_err(|e| format!("Failed to read expiry file: {}", e))?;
        let expiry: u64 = expiry_str.trim().parse()
            .map_err(|e| format!("Failed to parse expiry: {}", e))?;
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        if now > expiry {
            // Tokens expired, clean up
            let _ = fs::remove_file(&tokens_path);
            let _ = fs::remove_file(&expiry_path);
            return Err("Tokens expired".to_string());
        }
    }
    
    // Load tokens
    if !tokens_path.exists() {
        return Err("No tokens file found".to_string());
    }
    
    let tokens_json = fs::read_to_string(&tokens_path)
        .map_err(|e| format!("Failed to read tokens file: {}", e))?;
    
    let tokens: AuthTokens = serde_json::from_str(&tokens_json)
        .map_err(|e| format!("Failed to parse tokens: {}", e))?;
    
    Ok(tokens)
}

#[cfg(not(target_arch = "wasm32"))]
async fn clear_tokens_file() -> Result<(), String> {
    use std::fs;
    
    let app_dir = get_app_data_dir()?;
    let tokens_path = app_dir.join("auth_tokens.json");
    let expiry_path = app_dir.join("auth_expiry.txt");
    
    if tokens_path.exists() {
        fs::remove_file(&tokens_path)
            .map_err(|e| format!("Failed to remove tokens file: {}", e))?;
    }
    
    if expiry_path.exists() {
        fs::remove_file(&expiry_path)
            .map_err(|e| format!("Failed to remove expiry file: {}", e))?;
    }
    
    tracing::info!("Token files cleared");
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn get_app_data_dir() -> Result<std::path::PathBuf, String> {
    use std::env;
    
    #[cfg(target_os = "android")]
    {
        // For Android, use app-specific storage
        env::var("ANDROID_DATA")
            .map(|data| std::path::PathBuf::from(data).join("angel_trading"))
            .or_else(|_| {
                env::var("HOME")
                    .map(|home| std::path::PathBuf::from(home).join(".angel_trading"))
            })
            .map_err(|_| "Failed to determine app data directory".to_string())
    }
    
    #[cfg(target_os = "ios")]
    {
        // For iOS, use Documents directory
        env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join("Documents").join("angel_trading"))
            .map_err(|_| "Failed to determine app data directory".to_string())
    }
    
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // For desktop platforms
        env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".angel_trading"))
            .or_else(|_| {
                env::var("USERPROFILE")
                    .map(|profile| std::path::PathBuf::from(profile).join(".angel_trading"))
            })
            .map_err(|_| "Failed to determine app data directory".to_string())
    }
}

// Provider component for auth context
#[component]
pub fn AuthProvider(children: Element) -> Element {
    let auth_context = AuthContext::new();

    // Initialize auth state from storage on mount
    use_effect({
        let mut auth_context = auth_context.clone();
        move || {
            spawn(async move {
                tracing::info!("Initializing auth from storage...");
                let loaded_state = load_auth_from_storage().await;
                auth_context.state.set(loaded_state);
            });
        }
    });

    use_context_provider(|| auth_context);

    rsx! {
        {children}
    }
}
