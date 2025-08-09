use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

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

// Storage utilities using web-sys localStorage
async fn store_auth_tokens(tokens: &AuthTokens) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;
        let window = window().ok_or("No window available")?;
        let storage = window
            .local_storage()
            .map_err(|e| format!("Failed to access localStorage: {:?}", e))?
            .ok_or("localStorage not available")?;
        
        let tokens_json = serde_json::to_string(tokens)
            .map_err(|e| format!("Failed to serialize tokens: {}", e))?;
        
        storage
            .set_item(TOKEN_STORAGE_KEY, &tokens_json)
            .map_err(|e| format!("Failed to store tokens: {:?}", e))?;
    }
    Ok(())
}

async fn load_auth_from_storage() -> AuthState {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;
        if let Some(window) = window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(Some(tokens_json)) = storage.get_item(TOKEN_STORAGE_KEY) {
                    if let Ok(tokens) = serde_json::from_str::<AuthTokens>(&tokens_json) {
                        return AuthState::Authenticated(tokens);
                    } else {
                        // Clear invalid tokens
                        let _ = storage.remove_item(TOKEN_STORAGE_KEY);
                    }
                }
            }
        }
    }
    AuthState::Unauthenticated
}

async fn clear_auth_tokens() -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;
        let window = window().ok_or("No window available")?;
        let storage = window
            .local_storage()
            .map_err(|e| format!("Failed to access localStorage: {:?}", e))?
            .ok_or("localStorage not available")?;
        
        storage
            .remove_item(TOKEN_STORAGE_KEY)
            .map_err(|e| format!("Failed to clear tokens: {:?}", e))?;
    }
    Ok(())
}

// Provider component for auth context
#[component]
pub fn AuthProvider(children: Element) -> Element {
    let mut auth_context = AuthContext::new();
    
    // Initialize auth state from storage on mount
    use_effect(move || {
        spawn(async move {
            let loaded_state = load_auth_from_storage().await;
            auth_context.state.set(loaded_state);
        });
    });
    
    use_context_provider(|| auth_context);
    
    rsx! {
        {children}
    }
}