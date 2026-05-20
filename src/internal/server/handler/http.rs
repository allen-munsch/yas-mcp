// src/internal/server/handler/http.rs

use axum::{
    body::Body,
    // Removed extract::State
    http::{Request, StatusCode}, // Removed HeaderMap
    middleware::{self, Next},
    response::IntoResponse,
    routing::get,
    Router,
};
// Removed std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::{debug, info}; // Removed warn

/// Handler manages HTTP request handling and middleware configuration
pub struct Handler {
    auth_enabled: bool,
}

impl Handler {
    /// Create a new HTTP handler
    pub fn new(auth_enabled: bool) -> Self {
        Self { auth_enabled }
    }

    /// Create an HTTP handler with the appropriate middleware stack
    pub fn create_http_router(&self) -> Router {
        let mut router = Router::new()
            .route("/health", get(|| async { "OK" }))
            .layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(Self::log_requests))
                    .layer(CorsLayer::permissive()),
            );

        // Add authentication routes if enabled
        if self.auth_enabled {
            router = router
                .route("/auth/login", get(Self::auth_login))
                .route("/auth/callback", get(Self::auth_callback));
            info!("Authentication routes registered");
        }

        info!(
            "HTTP handler created with auth enabled: {}",
            self.auth_enabled
        );
        router
    }

    /// Authentication login endpoint
    async fn auth_login() -> impl IntoResponse {
        // TODO: Implement OAuth2 login redirect
        "Auth login endpoint - not yet implemented"
    }

    /// OAuth2 callback endpoint
    async fn auth_callback() -> impl IntoResponse {
        // TODO: Implement OAuth2 callback handling
        "Auth callback endpoint - not yet implemented"
    }

    /// Authentication middleware
    // async fn auth_middleware(
    //     request: Request<Body>,
    //     next: Next, // Removed <Body> generic
    // ) -> Result<impl IntoResponse, (StatusCode, String)> {
    //     // Extract and validate authentication headers
    //     let headers = request.headers();

    //     // Check for Authorization header
    //     if let Some(auth_header) = headers.get("authorization") {
    //         if let Ok(auth_str) = auth_header.to_str() {
    //             debug!("Auth header present: {}", auth_str);
    //             // TODO: Validate token
    //         }
    //     } else {
    //         debug!("No auth header present");
    //     }

    //     // For now, allow all requests - implement proper auth later
    //     let response = next.run(request).await;
    //     Ok(response)
    // }
    #[allow(clippy::empty_line_after_doc_comments)]
    /// Middleware to log HTTP requests
    async fn log_requests(
        request: Request<Body>,
        next: Next, // Removed <Body> generic
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        let method = request.method().clone();
        let uri = request.uri().clone();
        let version = request.version();

        debug!("→ {} {} {:?}", method, uri, version);

        let response = next.run(request).await;

        let status = response.status();
        debug!("← {} {}", status, uri);

        Ok(response)
    }
}

impl Default for Handler {
    fn default() -> Self {
        Self::new(false)
    }
}
