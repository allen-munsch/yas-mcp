// src/internal/server/server.rs

use crate::internal::mcp::registry::ToolRegistry;
use crate::internal::server::tool::ToolHandler;
use anyhow::{Context, Result};
use rmcp::{
    model::*,
    service::RequestContext,
    transport::sse_server::{SseServer, SseServerConfig},
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
};
use std::collections::HashMap;
use std::process;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};

use crate::internal::config::{AppConfig, AuthType, ServerMode};
use crate::internal::parser::_parser::SwaggerParser;
use crate::internal::parser::adjuster::Adjuster;
use crate::internal::parser::types::Parser;
use crate::internal::requester::HttpRequester;

/// Server represents the MCP server instance that handles tool management,
/// authentication, and request processing. It supports multiple operation modes
/// including SSE, HTTP, and STDIO.
#[derive(Clone)]
pub struct Server {
    pub config: AppConfig,
    parser: Arc<tokio::sync::Mutex<Box<dyn Parser>>>,
    requester: HttpRequester,
    pub tool_handler: Arc<tokio::sync::Mutex<ToolHandler>>,
}

// Implement ServerHandler trait (your existing implementation is fine)
impl ServerHandler for Server {
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tool_handler = self.tool_handler.lock().await;
        let tools = tool_handler.list_tool_metadata();

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.as_ref();

        let tool_handler = self.tool_handler.lock().await;
        if let Some(executor) = tool_handler.get_executor(tool_name) {
            let executor = Arc::clone(&executor);
            drop(tool_handler);

            // Create a CallToolRequest from the params
            let call_request = CallToolRequest {
                method: CallToolRequestMethod,
                params: request,
                extensions: Extensions::default(),
            };

            let future = executor(call_request);
            let result = future.await.map_err(|e| McpError {
                code: ErrorCode(-32600),
                message: e.to_string().into(),
                data: None,
            })?;

            Ok(result)
        } else {
            Err(McpError {
                code: ErrorCode(-32601),
                message: format!("Tool '{}' not found", tool_name).into(),
                data: None,
            })
        }
    }

    fn get_info(&self) -> ServerInfo {
        InitializeResult {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: self.config.server.name.clone(),
                version: self.config.server.version.clone(),
                icons: None,
                title: None,
                website_url: None,
            },
            instructions: Some(
                "OpenAPI MCP Server - exposes OpenAPI/Swagger endpoints as MCP tools".into(),
            ),
        }
    }
}

impl Server {
    /// Create a new MCP server instance with the provided configuration.
    pub async fn new(
        config: AppConfig,
        parser: Box<dyn Parser>,
        requester: HttpRequester,
    ) -> Result<Self> {
        if config.swagger_file.is_empty() {
            error!("Swagger file path cannot be empty");
            process::exit(1);
        }

        let auth_enabled = config.oauth.as_ref().map(|o| o.enabled).unwrap_or(false);
        let registry = Arc::new(ToolRegistry::new());
        let tool_handler = ToolHandler::new(auth_enabled, registry);

        let server = Self {
            config,
            parser: Arc::new(tokio::sync::Mutex::new(parser)),
            requester,
            tool_handler: Arc::new(tokio::sync::Mutex::new(tool_handler)),
        };

        Ok(server)
    }

    /// Setup tools from parsed OpenAPI specification
    pub async fn setup_tools(&self) -> Result<()> {
        info!("Loading adjustments and parsing OpenAPI spec...");

        let mut parser = self.parser.lock().await;
        parser
            .init(
                &self.config.swagger_file,
                self.config.adjustments_file.as_deref(),
            )
            .context("Failed to initialize parser")?;

        let route_tools = parser.get_route_tools().to_vec();

        let mut tool_handler = self.tool_handler.lock().await;

        for route_tool in route_tools {
            let executor = self
                .requester
                .build_route_executor(&route_tool.route_config)
                .with_context(|| {
                    format!(
                        "Failed to build executor for route: {}",
                        route_tool.route_config.path
                    )
                })?;

            let tool_name = route_tool.tool.name.clone().to_owned();
            let handler = tool_handler.create_handler(&tool_name, executor);
            tool_handler.register_tool(
                &tool_name,
                route_tool.tool.to_owned(),
                handler.clone(),
            );

            info!(
                "Registered tool: {} {} -> {}",
                route_tool.route_config.method, route_tool.route_config.path, tool_name
            );
        }

        info!(
            "Successfully registered {} tools",
            tool_handler.tool_count()
        );
        Ok(())
    }


    /// Serve in STDIO mode (primary MCP mode)
    async fn serve_stdio(&self) -> Result<()> {
        info!("Starting STDIO server with {} tools", self.tool_count());
        info!("MCP Server ready! Tools available: {}", self.tool_count());

        // Use stdin/stdout directly with IntoTransport - this should work
        let transport = (tokio::io::stdin(), tokio::io::stdout());
        let service = self.clone().serve(transport).await?;

        // Wait for the service to complete
        service.waiting().await?;

        Ok(())
    }

    /// Serve in HTTP mode - proper MCP JSON-RPC over HTTP with streaming support
    async fn serve_http(&self) -> Result<()> {
        use axum::{
            extract::State,
            http::StatusCode,
            response::{
                sse::{Event, KeepAlive},
                IntoResponse, Response,
            },
            routing::{get, post},
            Json,
        };
        use serde_json::Value;
        use std::convert::Infallible;

        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        info!(
            "Starting HTTP MCP server on {} with {} tools",
            addr,
            self.tool_count()
        );

        #[derive(Clone)]
        struct AppState {
            server: Server,
            sessions: Arc<tokio::sync::RwLock<HashMap<String, SessionData>>>,
        }

        #[derive(Clone)]
        struct SessionData {
            // Store session-specific data if needed
            #[allow(dead_code)]
            created_at: std::time::Instant,
        }

        let state = AppState {
            server: self.clone(),
            sessions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        };

        // HTTP handler for standard MCP JSON-RPC requests
        async fn handle_mcp_request(
            State(app_state): State<AppState>,
            headers: axum::http::HeaderMap,
            Json(payload): Json<Value>,
        ) -> impl IntoResponse {
            // Extract session ID from headers if present
            let session_id = headers
                .get("x-session-id")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // Parse the JSON-RPC request
            let method = payload.get("method").and_then(|m| m.as_str());
            let id = payload.get("id").cloned();

            let response = match method {
                Some("initialize") => {
                    // Handle initialize request
                    let info = app_state.server.get_info();

                    // Create a new session if this is initialization
                    let new_session_id = uuid::Uuid::new_v4().to_string();
                    {
                        let mut sessions = app_state.sessions.write().await;
                        sessions.insert(
                            new_session_id.clone(),
                            SessionData {
                                created_at: std::time::Instant::now(),
                            },
                        );
                    }

                    let mut response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "result": info,
                        "id": id
                    });

                    // Add session ID to response
                    if let Some(obj) = response.as_object_mut() {
                        obj.insert("sessionId".to_string(), serde_json::json!(new_session_id));
                    }

                    response
                }
                Some("tools/list") => {
                    let tools = app_state.server.list_tools_simple().await;
                    match tools {
                        Ok(tools) => serde_json::json!({
                            "jsonrpc": "2.0",
                            "result": tools,
                            "id": id
                        }),
                        Err(e) => serde_json::json!({
                            "jsonrpc": "2.0",
                            "error": {
                                "code": e.code.0,
                                "message": e.message
                            },
                            "id": id
                        }),
                    }
                }
                Some("tools/call") => {
                    let params = payload.get("params");
                    match params.and_then(|p| {
                        serde_json::from_value::<CallToolRequestParam>(p.clone()).ok()
                    }) {
                        Some(params) => {
                            let result = app_state.server.call_tool_simple(params).await;
                            match result {
                                Ok(tool_result) => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "result": tool_result,
                                    "id": id
                                }),
                                Err(e) => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "error": {
                                        "code": e.code.0,
                                        "message": e.message
                                    },
                                    "id": id
                                }),
                            }
                        }
                        None => serde_json::json!({
                            "jsonrpc": "2.0",
                            "error": {
                                "code": -32602,
                                "message": "Invalid params"
                            },
                            "id": id
                        }),
                    }
                }
                Some("notifications/initialized") => {
                    // Handle initialized notification
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "result": null,
                        "id": id
                    })
                }
                Some("ping") => {
                    // Handle ping for keep-alive
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "result": {},
                        "id": id
                    })
                }
                _ => serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {:?}", method)
                    },
                    "id": id
                }),
            };

            // Return response with session ID header
            let mut headers = axum::http::HeaderMap::new();
            if let Some(sid) = session_id {
                if let Ok(header_value) = axum::http::HeaderValue::from_str(&sid) {
                    headers.insert("x-session-id", header_value);
                }
            }

            (StatusCode::OK, headers, Json(response))
        }

        // SSE endpoint for streaming responses (optional but useful for progress updates)
        async fn handle_sse_stream(
            State(app_state): State<AppState>,
            headers: axum::http::HeaderMap,
        ) -> Response {
            let session_id = headers
                .get("x-session-id")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // Verify session exists
            if let Some(session_id) = session_id {
                let sessions = app_state.sessions.read().await;
                if !sessions.contains_key(&session_id) {
                    return (StatusCode::UNAUTHORIZED, "Invalid session").into_response();
                }
            } else {
                return (StatusCode::BAD_REQUEST, "Missing session ID").into_response();
            }

            // Create a stream for SSE events
            let stream = async_stream::stream! {
                // Send keep-alive events
                loop {
                    yield Ok::<_, Infallible>(
                        Event::default()
                            .event("ping")
                            .data("{}")
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                }
            };

            axum::response::Sse::new(stream)
                .keep_alive(KeepAlive::default())
                .into_response()
        }

        // Session cleanup endpoint
        async fn handle_session_delete(
            State(app_state): State<AppState>,
            headers: axum::http::HeaderMap,
        ) -> impl IntoResponse {
            let session_id = headers
                .get("x-session-id")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            if let Some(session_id) = session_id {
                let mut sessions = app_state.sessions.write().await;
                sessions.remove(&session_id);
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "status": "session deleted"
                    })),
                )
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Missing session ID"
                    })),
                )
            }
        }

        // Create router with all endpoints
        let handler = crate::internal::server::handler::Handler::new(
            self.config.endpoint.auth_type != AuthType::None,
        );
        let health_router = handler.create_http_router();

        let app = axum::Router::new()
            .route("/mcp", post(handle_mcp_request))
            .route("/sse", get(handle_sse_stream))
            .route("/session", axum::routing::delete(handle_session_delete))
            .with_state(state)
            .merge(health_router);

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("Failed to bind to address: {}", addr))?;

        info!("HTTP MCP server listening on {}", addr);
        info!("Endpoints:");
        info!("  - POST http://{}/mcp - Main JSON-RPC endpoint", addr);
        info!(
            "  - GET  http://{}/sse - Server-Sent Events stream (optional)",
            addr
        );
        info!("  - DELETE http://{}/session - Session cleanup", addr);
        info!("  - GET  http://{}/health - Health check", addr);

        axum::serve(listener, app)
            .await
            .context("HTTP server failed")?;

        Ok(())
    }
    /// Simplified list_tools for HTTP mode that doesn't require RequestContext
    async fn list_tools_simple(&self) -> Result<ListToolsResult, McpError> {
        let tool_handler = self.tool_handler.lock().await;
        let tools = tool_handler.list_tool_metadata();

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    /// Simplified call_tool for HTTP mode that doesn't require RequestContext
    async fn call_tool_simple(
        &self,
        request: CallToolRequestParam,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.as_ref();

        let tool_handler = self.tool_handler.lock().await;
        if let Some(executor) = tool_handler.get_executor(tool_name) {
            let executor = Arc::clone(&executor);
            drop(tool_handler);

            // Create a CallToolRequest from the params
            let call_request = CallToolRequest {
                method: CallToolRequestMethod,
                params: request,
                extensions: Extensions::default(),
            };

            let future = executor(call_request);
            let result = future.await.map_err(|e| McpError {
                code: ErrorCode(-32600),
                message: e.to_string().into(),
                data: None,
            })?;

            Ok(result)
        } else {
            Err(McpError {
                code: ErrorCode(-32601),
                message: format!("Tool '{}' not found", tool_name).into(),
                data: None,
            })
        }
    }

    /// Serve in SSE mode - use RMCP's SseServer
    async fn serve_sse(&self) -> Result<()> {
        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        info!(
            "Starting SSE MCP server on {} with {} tools",
            addr,
            self.tool_count()
        );

        // Parse the bind address
        let bind_addr: std::net::SocketAddr = addr
            .parse()
            .with_context(|| format!("Failed to parse bind address: {}", addr))?;

        // Create cancellation token
        let ct = tokio_util::sync::CancellationToken::new();

        // Create SSE server configuration
        let sse_config = SseServerConfig {
            bind: bind_addr,
            sse_path: "/sse".to_string(),
            post_path: "/message".to_string(),
            ct: ct.clone(),
            sse_keep_alive: Some(std::time::Duration::from_secs(30)),
        };

        // Create the SSE server - it returns (server, router)
        let (sse_server, sse_router) = SseServer::new(sse_config);

        info!("SSE MCP server configured");
        info!("SSE endpoint: GET http://{}/sse", addr);
        info!("Message endpoint: POST http://{}/message", addr);

        // Clone self for the service factory
        let server_clone = self.clone();

        // Connect the MCP service to the SSE server
        let service_ct = sse_server.with_service(move || server_clone.clone());

        // Spawn task to serve the SSE router
        // The sse_router is already a complete Axum 0.8 router with /sse and /message
        let http_handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(&bind_addr)
                .await
                .expect("Failed to bind listener");

            info!("HTTP server listening on {}", bind_addr);

            // Use the axum 0.8 serve function (via the sse_router)
            axum::serve(listener, sse_router)
                .await
                .expect("Server failed");
        });

        // Set up graceful shutdown
        let shutdown_ct = ct.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C handler");
            info!("Received shutdown signal, stopping SSE server");
            shutdown_ct.cancel();
        });

        // Wait for either the service to complete or the HTTP server to stop
        tokio::select! {
            _ = service_ct.cancelled() => {
                info!("SSE service cancelled");
            }
            result = http_handle => {
                match result {
                    Ok(_) => info!("HTTP server stopped"),
                    Err(e) => error!("HTTP server task error: {}", e),
                }
            }
        }

        info!("SSE server shutdown complete");
        Ok(())
    }

    /// Start the server in the configured mode
    pub async fn start(&self) -> Result<()> {
        self.setup_tools().await?;

        info!(
            "Starting server in {:?} mode, version: {} with {} tools",
            self.config.server.mode,
            self.config.server.version,
            self.tool_count()
        );

        match self.config.server.mode {
            ServerMode::Stdio => self.serve_stdio().await,
            ServerMode::Http => self.serve_http().await,
            ServerMode::Sse => self.serve_sse().await,
        }
    }

    /// Start the server with graceful shutdown handling
    pub async fn start_with_graceful_shutdown(&self) -> Result<()> {
        let shutdown = async {
            signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
            info!("Received shutdown signal");
        };

        tokio::select! {
            result = self.start() => {
                result
            }
            _ = shutdown => {
                info!("Shutting down gracefully");
                Ok(())
            }
        }
    }

    /// Get the number of registered tools
    pub fn tool_count(&self) -> usize {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.tool_handler.lock().await.tool_count() })
        })
    }
    /// Get the underlying tool registry.
    pub async fn get_tool_registry(&self) -> Arc<ToolRegistry> {
        let tool_handler_guard = self.tool_handler.lock().await;
        tool_handler_guard.registry()
    }
}

// Helper function to create server with dependencies
pub async fn create_server(config: AppConfig) -> Result<Server> {
    let adjuster = Adjuster::new();
    let parser = Box::new(SwaggerParser::new(adjuster));

    let requester =
        HttpRequester::new(&config.endpoint).context("Failed to create HTTP requester")?;

    Server::new(config, parser, requester).await
}
