//! gRPC Transport — Production
//!
//! Implements MCP-over-gRPC using the protobuf schema from `proto/mcp.proto`.
//! The MCP community has not yet standardized gRPC as a transport; when they
//! do, the `.proto` file will be updated to match the official spec.
//!
//! # Design
//!
//! This transport implements the same `McpTransport` trait as HTTP and STDIO,
//! so the tool registry, auth middleware, metrics, caching, and circuit breakers
//! all work identically regardless of transport.
//!
//! # Enabling
//!
//! ```bash
//! yas-mcp --mode grpc --grpc-port 50051
//! ```
//!
//! # When the standard arrives
//!
//! 1. Replace `proto/mcp.proto` with the official schema
//! 2. Regenerate Rust types: `cargo build`
//! 3. Update method names in the service implementation below
//!
//! The rest of yas-mcp (tool registry, auth, metrics, etc.) stays unchanged.

use crate::internal::mcp::registry::ToolRegistry;
use crate::internal::server::tool::ToolHandler;
use std::sync::Arc;

// ── Transport abstraction — implemented by HTTP, STDIO, and gRPC
#[async_trait::async_trait]
pub trait McpTransport: Send + Sync {
    /// Start serving — blocks until shutdown
    async fn serve(
        &self,
        tool_handler: Arc<tokio::sync::Mutex<ToolHandler>>,
        registry: Arc<ToolRegistry>,
    ) -> anyhow::Result<()>;

    /// Human-readable name for logging
    fn name(&self) -> &str;

    /// Whether this transport is experimental
    fn is_experimental(&self) -> bool {
        false
    }
}

/// Configuration for the gRPC transport
#[derive(Debug, Clone)]
pub struct GrpcConfig {
    pub port: u16,
    pub host: String,
    /// When true, logs a warning that this transport is experimental
    pub experimental_notice: bool,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            port: 50051,
            host: "0.0.0.0".into(),
            experimental_notice: true,
        }
    }
}

/// Build a gRPC transport from config.
///
pub fn create_grpc_transport(config: GrpcConfig) -> anyhow::Result<Box<dyn McpTransport>> {
    crate::internal::transport::grpc_server::GrpcServer::new(config)
        .map(|s| Box::new(s) as Box<dyn McpTransport>)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_config_defaults() {
        let config = GrpcConfig::default();
        assert_eq!(config.port, 50051);
        assert_eq!(config.host, "0.0.0.0");
        assert!(config.experimental_notice);
    }

    #[test]
    fn test_create_grpc_transport() {
        let result = create_grpc_transport(GrpcConfig::default());
        assert!(result.is_ok());
    }
}
