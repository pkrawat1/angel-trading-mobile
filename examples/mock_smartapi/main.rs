//! Serves the SmartAPI stand-in on 127.0.0.1:9100, or on the port given as
//! the first argument, and prints each request it answers.
//!
//! ```sh
//! cargo run --example mock_smartapi --no-default-features --features server
//! SMARTAPI_BASE_URL=http://127.0.0.1:9100 dx serve --platform ios --addr 127.0.0.1
//! ```
//!
//! Sign in with any client code and password and any six-digit TOTP except
//! 000000. Orders change only the mock's in-memory order book. These client
//! codes sign in to a failure instead of the fixtures:
//!
//! - `EXPIRED`: every call fails with Angel One's invalid-session error.
//! - `BROKER-DOWN`: every call fails with a broker error.
//! - `HTML`: every call gets an HTML error page.
//! - `EMPTY`: every call succeeds with no data.
//! - `RATE-LIMITED`, `FLAKY`: the first call to each endpoint fails.

mod server;

use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(9100);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("Mock SmartAPI on http://{}", listener.local_addr()?);
    let app = server::router(Arc::new(server::Mock::default())).layer(middleware::from_fn(log));
    axum::serve(listener, app).await
}

async fn log(request: Request, next: Next) -> Response {
    let line = format!("{} {}", request.method(), request.uri().path());
    let response = next.run(request).await;
    println!("{} {line}", response.status().as_u16());
    response
}
