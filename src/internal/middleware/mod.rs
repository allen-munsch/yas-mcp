//! Pluggable middleware framework — auth, transformation, and request/response hooks.
//!
//! yas-mcp supports custom middleware that orgs can plug in without modifying source.
//! Middleware runs in a defined pipeline order for each MCP tool invocation.
//!
//! ## Auth Middleware
//!
//! Built-in providers: OIDC, OAuth2, API Key, Basic Auth, mTLS, Header Passthrough.
//! Custom: orgs implement the `AuthMiddleware` trait and register via config.
//!
//! ## Pipeline Middleware
//!
//! Response processing stages: Paginate, Filter, Map, Join, Aggregate, Cache.
//! Declared in YAML, composed into pipelines, applied per-tool or per-route.

pub mod auth;
pub mod pipeline;

// Re-export key types
pub use auth::{AuthContext, AuthMiddleware, AuthProviderType};
pub use pipeline::{
    Pipeline, PipelineConfig, PipelineStage, StageOutput, StageType,
};
