//! Authentication Middleware
//!
//! Tower Layer-based auth middleware that chains multiple
//! AuthProvider implementations. Each provider can match specific
//! routes via glob patterns, enabling multi-tenant auth.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::{debug, warn};

use crate::internal::auth::provider::{AuthIdentity, AuthProvider};

/// A chain of auth providers, evaluated in order.
/// The first provider that matches the route AND successfully authenticates wins.
pub struct AuthMiddleware {
    providers: Vec<Box<dyn AuthProvider>>,
    /// If true, requests with no matching provider are allowed through.
    /// If false, they receive 401.
    allow_unmatched: bool,
}

impl AuthMiddleware {
    /// Create a new auth middleware with the given providers
    pub fn new(providers: Vec<Box<dyn AuthProvider>>, allow_unmatched: bool) -> Self {
        Self {
            providers,
            allow_unmatched,
        }
    }

    /// Create an auth middleware that allows all requests (no auth)
    pub fn passthrough() -> Self {
        Self {
            providers: Vec::new(),
            allow_unmatched: true,
        }
    }

    /// Check if any provider is configured
    pub fn is_enabled(&self) -> bool {
        !self.providers.is_empty()
    }

    /// Number of configured providers
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// The actual middleware function
    pub async fn handle(
        self: Arc<Self>,
        request: Request<Body>,
        next: Next,
    ) -> Result<Response, (StatusCode, String)> {
        // If no providers configured, pass through
        if self.providers.is_empty() && self.allow_unmatched {
            return Ok(next.run(request).await);
        }

        let path = request.uri().path();

        // Extract headers into a HashMap for provider consumption
        let headers: HashMap<String, String> = request
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_lowercase(), v.to_string()))
            })
            .collect();

        // Try each provider in order
        for provider in &self.providers {
            if !provider.matches_route(path) {
                debug!(
                    "Auth provider '{}' does not match route: {}",
                    provider.provider_type(),
                    path
                );
                continue;
            }

            match provider.authenticate(&headers) {
                Ok(Some(identity)) => {
                    debug!(
                        "Authenticated via '{}': subject={}",
                        identity.provider, identity.subject
                    );

                    // Inject identity into request extensions
                    let mut request = request;
                    request.extensions_mut().insert(Arc::new(identity));

                    return Ok(next.run(request).await);
                }
                Ok(None) => {
                    // Provider matched the route but found no credentials.
                    // If this provider is optional, continue to next.
                    // If required, reject.
                    warn!(
                        "Auth provider '{}' matched route '{}' but no valid credentials",
                        provider.provider_type(),
                        path
                    );
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        format!(
                            "Authentication required by '{}'",
                            provider.provider_type()
                        ),
                    ));
                }
                Err(e) => {
                    warn!(
                        "Auth provider '{}' error for route '{}': {}",
                        provider.provider_type(),
                        path,
                        e
                    );
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        format!("Authentication failed: {e}"),
                    ));
                }
            }
        }

        // No provider matched
        if self.allow_unmatched {
            Ok(next.run(request).await)
        } else {
            Err((
                StatusCode::UNAUTHORIZED,
                "No authentication provider matched this route".into(),
            ))
        }
    }
}

/// Helper to extract an AuthIdentity from request extensions
pub fn get_identity(request: &Request<Body>) -> Option<Arc<AuthIdentity>> {
    request.extensions().get::<Arc<AuthIdentity>>().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::auth::provider::AuthProvider;
    use anyhow::Result;
    use axum::{body::Body, http::Request, middleware, routing::get, Router};
    use tower::ServiceExt;

    /// A test provider that matches specific paths
    struct TestProvider;

    impl AuthProvider for TestProvider {
        fn provider_type(&self) -> &str {
            "test"
        }

        fn authenticate(
            &self,
            headers: &HashMap<String, String>,
        ) -> Result<Option<AuthIdentity>> {
            if headers.get("authorization").map(|s| s.as_str()) == Some("Bearer valid") {
                Ok(Some(AuthIdentity {
                    subject: "test-user".into(),
                    name: None,
                    email: None,
                    provider: "test".into(),
                    claims: HashMap::new(),
                }))
            } else {
                Ok(None)
            }
        }

        fn matches_route(&self, path: &str) -> bool {
            path.starts_with("/api/protected")
        }
    }

    #[tokio::test]
    async fn test_passthrough_middleware() {
        let middleware = Arc::new(AuthMiddleware::passthrough());

        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn({
                let mw = middleware.clone();
                move |req, next| {
                    let mw = mw.clone();
                    async move { mw.handle(req, next).await }
                }
            }));

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_valid() {
        let middleware = Arc::new(AuthMiddleware::new(
            vec![Box::new(TestProvider)],
            true,
        ));

        let app = Router::new()
            .route("/api/protected/data", get(|| async { "ok" }))
            .layer(middleware::from_fn({
                let mw = middleware.clone();
                move |req, next| {
                    let mw = mw.clone();
                    async move { mw.handle(req, next).await }
                }
            }));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/protected/data")
                    .header("Authorization", "Bearer valid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_invalid_token() {
        let middleware = Arc::new(AuthMiddleware::new(
            vec![Box::new(TestProvider)],
            true,
        ));

        let app = Router::new()
            .route("/api/protected/data", get(|| async { "ok" }))
            .layer(middleware::from_fn({
                let mw = middleware.clone();
                move |req, next| {
                    let mw = mw.clone();
                    async move { mw.handle(req, next).await }
                }
            }));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/protected/data")
                    .header("Authorization", "Bearer wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_unmatched_route_allowed() {
        let middleware = Arc::new(AuthMiddleware::new(
            vec![Box::new(TestProvider)],
            true, // allow unmatched
        ));

        let app = Router::new()
            .route("/api/public/data", get(|| async { "ok" }))
            .layer(middleware::from_fn({
                let mw = middleware.clone();
                move |req, next| {
                    let mw = mw.clone();
                    async move { mw.handle(req, next).await }
                }
            }));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/public/data")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}