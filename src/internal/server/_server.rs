// src/internal/server/_server.rs

use crate::internal::mcp::registry::ToolRegistry;
use crate::internal::server::tool::ToolHandler;
use crate::internal::mcp::processor::McpProcessor;
use crate::internal::transport::runner::TransportRunner;
use crate::internal::transport::stdio::StdioTransport;

use anyhow::{Context, Result};
use rmcp::{
    model::*,
    service::RequestContext,
    ErrorData as McpError,
    RoleServer,
    ServerHandler,
};
use std::process;
use std::sync::Arc;
use tracing::{error, info};

use crate::internal::config::{AppConfig, ServerMode};
use crate::internal::parser::_parser::SwaggerParser;
use crate::internal::parser::adjuster::Adjuster;
use crate::internal::parser::types::Parser;
use crate::internal::requester::HttpRequester;

#[derive(Clone)]
pub struct Server {
    pub config: AppConfig,
    parser: Arc<tokio::sync::Mutex<Box<dyn Parser>>>,
    requester: HttpRequester,
    pub tool_handler: Arc<tokio::sync::Mutex<ToolHandler>>,
}

// Implement ServerHandler trait (Still needed for internal logic if called directly)
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
            meta: None,
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
            instructions: Some("OpenAPI MCP Server".into()),
        }
    }
}

impl Server {
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

            let tool_name = route_tool.tool.name.clone().clone();
            let handler = tool_handler.create_handler(&tool_name, executor);
            tool_handler.register_tool(&tool_name, route_tool.tool.to_owned(), handler.clone());

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

    async fn serve_stdio(&self) -> Result<()> {
        // Logs go to stderr, so this is safe
        info!("Starting STDIO server with {} tools", self.tool_count());

        // 1. Raw Transport
        let transport = StdioTransport::new();

        // 2. Get Registry
        let tool_handler = self.tool_handler.lock().await;
        let registry = tool_handler.registry();
        drop(tool_handler);

        // 3. Clean Processor (No 'rmcp' runtime logic)
        let processor = Arc::new(McpProcessor::new(self, registry));

        // 4. Run loop
        let mut runner = TransportRunner::new(transport, processor);
        runner
            .run()
            .await
            .map_err(|e| anyhow::anyhow!("Transport error: {}", e))
    }

    /// Serve in HTTP mode - proper MCP JSON-RPC over HTTP
    async fn serve_http(&self) -> Result<()> {
        use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::{post, get}, Json};
        use serde_json::Value;
        async fn health() -> impl IntoResponse {
            StatusCode::OK
        }

        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        info!(
            "Starting HTTP MCP server on {} with {} tools",
            addr,
            self.tool_count()
        );

        // 1. Define State
        #[derive(Clone)]
        struct AppState {
            server: Server,
        }

        let state = AppState {
            server: self.clone(),
        };

        // 2. Define the JSON-RPC Handler
        async fn handle_mcp_request(
            State(app_state): State<AppState>,
            Json(payload): Json<Value>,
        ) -> impl IntoResponse {
            let method = payload.get("method").and_then(|m| m.as_str());
            let id = payload.get("id").cloned();

            // Use the clean simplified logic we wrote
            let response = match method {
                Some("initialize") => {
                    let info = app_state.server.get_info();
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "result": info,
                        "id": id
                    })
                }
                Some("tools/list") => {
                    // Uses the simple list logic (no context needed)
                    let tools = app_state.server.list_tools_simple().await;
                    match tools {
                        Ok(result) => serde_json::json!({
                            "jsonrpc": "2.0",
                            "result": result,
                            "id": id
                        }),
                        Err(e) => serde_json::json!({
                            "jsonrpc": "2.0",
                            "error": { "code": e.code.0, "message": e.message },
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
                                Ok(res) => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "result": res,
                                    "id": id
                                }),
                                Err(e) => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "error": { "code": e.code.0, "message": e.message },
                                    "id": id
                                }),
                            }
                        }
                        None => serde_json::json!({
                            "jsonrpc": "2.0",
                            "error": { "code": -32602, "message": "Invalid params" },
                            "id": id
                        }),
                    }
                }
                Some("notifications/initialized") | Some("ping") => {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "result": {},
                        "id": id
                    })
                }
                _ => serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": { "code": -32601, "message": "Method not found" },
                    "id": id
                }),
            };

            (StatusCode::OK, Json(response))
        }

        // 3. Build Router
        let app = axum::Router::new()
            .route("/health", get(health))
            .route("/mcp", post(handle_mcp_request))
            .with_state(state);

        // 4. Start Server
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("Failed to bind to address: {}", addr))?;

        info!("HTTP MCP server listening on {}", addr);
        info!("Endpoint: POST http://{}/mcp", addr);

        axum::serve(listener, app)
            .await
            .context("HTTP server failed")?;

        Ok(())
    }

    async fn list_tools_simple(&self) -> Result<ListToolsResult, McpError> {
        let tool_handler = self.tool_handler.lock().await;
        Ok(ListToolsResult {
            tools: tool_handler.list_tool_metadata(),
            next_cursor: None,
            meta: None, // Required for rmcp 0.12.0
        })
    }

    async fn call_tool_simple(
        &self,
        request: CallToolRequestParam,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.as_ref();
        let tool_handler = self.tool_handler.lock().await;

        if let Some(executor) = tool_handler.get_executor(tool_name) {
            let executor = Arc::clone(&executor);
            drop(tool_handler);

            let call_request = CallToolRequest {
                method: CallToolRequestMethod,
                params: request,
                extensions: Extensions::default(),
            };

            executor(call_request).await.map_err(|e| McpError {
                code: ErrorCode(-32600),
                message: e.to_string().into(),
                data: None,
            })
        } else {
            Err(McpError {
                code: ErrorCode(-32601),
                message: format!("Tool '{}' not found", tool_name).into(),
                data: None,
            })
        }
    }

    // --- SSE IS DEAD: Stubbed out ---
    async fn serve_sse(&self) -> Result<()> {
        error!("SSE mode is deprecated and removed. Please use HTTP or Stdio.");
        Err(anyhow::anyhow!("SSE mode not supported"))
    }

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

    pub async fn start_with_graceful_shutdown(&self) -> Result<()> {
        // Simple shutdown for Stdio (Ctrl+C kills the process anyway)
        self.start().await
    }

    pub fn tool_count(&self) -> usize {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.tool_handler.lock().await.tool_count() })
        })
    }

    pub async fn get_tool_registry(&self) -> Arc<ToolRegistry> {
        let tool_handler_guard = self.tool_handler.lock().await;
        tool_handler_guard.registry()
    }
}

pub async fn create_server(config: AppConfig) -> Result<Server> {
    let adjuster = Adjuster::new();
    let parser = Box::new(SwaggerParser::new(adjuster));
    let requester =
        HttpRequester::new(&config.endpoint).context("Failed to create HTTP requester")?;
    Server::new(config, parser, requester).await
}
