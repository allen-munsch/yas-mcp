pub mod handler;
pub mod tool;

use crate::internal::mcp::processor::McpProcessor;
use crate::internal::mcp::registry::ToolRegistry;
use crate::internal::server::tool::ToolHandler;
use crate::internal::transport::runner::TransportRunner;
use crate::internal::transport::stdio::StdioTransport;

use anyhow::{Context, Result};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, model::*, service::RequestContext};
use std::process;
use std::sync::Arc;
use tracing::{error, info};

use crate::internal::auth::token_store::ConnectorTokenCache;
use crate::internal::config::{AppConfig, ServerMode};
use crate::internal::parser::SwaggerParser;
use crate::internal::parser::adjuster::Adjuster;
use crate::internal::parser::types::Parser;
use crate::internal::requester::HttpRequester;
use crate::internal::requester::types::RouteConfig;

#[derive(Clone)]
pub struct Server {
    pub config: AppConfig,
    parser: Arc<tokio::sync::Mutex<Box<dyn Parser>>>,
    requester: HttpRequester,
    pub tool_handler: Arc<tokio::sync::Mutex<ToolHandler>>,
    pub route_configs: Arc<tokio::sync::Mutex<Vec<RouteConfig>>>,
    /// Connector token cache for WIMSE token exchange
    pub connector_tokens: ConnectorTokenCache,
}

// Implement ServerHandler trait (Still needed for internal logic if called directly)
impl ServerHandler for Server {
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tool_handler = self.tool_handler.lock().await;
        let tools = tool_handler.list_tool_metadata();

        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool_name = request.name.clone();

        let tool_handler = self.tool_handler.lock().await;
        if let Some(executor) = tool_handler.get_executor(&tool_name) {
            let executor = Arc::clone(&executor);
            drop(tool_handler);

            let future = executor(request);
            let result = future.await.map_err(|e| McpError {
                code: ErrorCode(-32600),
                message: e.to_string().into(),
                data: None,
            })?;

            Ok(CallToolResponse::Complete(result))
        } else {
            Err(McpError {
                code: ErrorCode(-32601),
                message: format!("Tool '{}' not found", tool_name).into(),
                data: None,
            })
        }
    }

    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info = info.with_server_info(Implementation::new(
            self.config.server.name.clone(),
            self.config.server.version.clone(),
        ));
        info.instructions = Some("OpenAPI MCP Server".into());
        info
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
            route_configs: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            connector_tokens: ConnectorTokenCache::new(),
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

        // Collect route configs for Agent Card generation and A2A
        let mut collected_configs = Vec::new();

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

            // Collect route config for Agent Card generation
            collected_configs.push(route_tool.route_config.clone());

            info!(
                "Registered tool: {} {} -> {}",
                route_tool.route_config.method, route_tool.route_config.path, tool_name
            );
        }

        info!(
            "Successfully registered {} tools",
            tool_handler.tool_count()
        );

        // Store route configs on the server for Agent Card generation
        *self.route_configs.lock().await = collected_configs;

        // Update active tools gauge
        crate::internal::telemetry::Metrics::get()
            .set_active_tools(tool_handler.tool_count() as f64);

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

    /// Serve in HTTP mode - proper MCP JSON-RPC over HTTP (with optional A2A)
    async fn serve_http(&self) -> Result<()> {
        use axum::{
            Json,
            extract::State,
            http::StatusCode,
            response::IntoResponse,
            routing::{get, post},
        };
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

        // Check if A2A is enabled
        let a2a_enabled = self.config.a2a.as_ref().map(|a| a.enabled).unwrap_or(false);
        let wimse_enabled = self
            .config
            .wimse
            .as_ref()
            .map(|w| w.enabled)
            .unwrap_or(false);
        if a2a_enabled {
            info!("A2A protocol enabled — agent-to-agent endpoints available");
        }

        // 1. Define State
        #[derive(Clone)]
        struct AppState {
            server: Server,
        }

        let state = AppState {
            server: self.clone(),
        };

        // Prepare A2A state if enabled
        let a2a_state = if a2a_enabled {
            let tool_handler = self.tool_handler.lock().await;
            let registry = tool_handler.registry();
            let route_configs = self.route_configs.lock().await.clone();
            drop(tool_handler);

            let task_store = Arc::new(crate::internal::a2a::TaskStore::new(
                self.config
                    .a2a
                    .as_ref()
                    .map(|a| a.max_concurrent_tasks)
                    .unwrap_or(100),
                self.config.a2a.as_ref().map(|a| a.task_ttl).unwrap_or(3600),
            ));

            Some(crate::internal::a2a::router::A2aState {
                config: self.config.clone(),
                tool_registry: registry,
                route_configs: Arc::new(route_configs),
                task_store,
            })
        } else {
            None
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
                        serde_json::from_value::<CallToolRequestParams>(p.clone()).ok()
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
                Some("notifications/initialized") => {
                    // Notifications have no id — don't send a response
                    return (StatusCode::OK, Json(serde_json::Value::Null));
                }
                Some("ping") => {
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

        // 3. Build secret store and auth middleware from config
        let secret_store = self.build_secret_store();
        let auth_middleware = self.build_auth_middleware(&secret_store);
        let auth_enabled = auth_middleware.is_enabled();
        if auth_enabled {
            info!(
                "Auth middleware enabled with {} provider(s)",
                auth_middleware.provider_count()
            );
        }

        // 4. Build Router (with optional auth + A2A routes)
        async fn metrics_handler() -> impl IntoResponse {
            let metrics = crate::internal::telemetry::Metrics::get();
            match metrics.encode() {
                Ok(text) => (
                    StatusCode::OK,
                    [("content-type", "text/plain; version=0.0.4")],
                    text,
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("content-type", "text/plain")],
                    e,
                ),
            }
        }

        async fn ai_catalog_handler(State(app_state): State<AppState>) -> impl IntoResponse {
            let tool_handler = app_state.server.tool_handler.lock().await;
            let registry = tool_handler.registry();
            let routes = app_state.server.route_configs.lock().await.clone();
            drop(tool_handler);

            let catalog = crate::internal::catalog::CatalogGenerator::generate(
                &app_state.server.config,
                &registry,
                &routes,
            );

            match serde_json::to_string_pretty(&catalog) {
                Ok(json) => (StatusCode::OK, [("content-type", "application/json")], json),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("content-type", "text/plain")],
                    format!("Failed to serialize catalog: {e}"),
                ),
            }
        }

        // WIMSE token exchange handler — validates platform JWTs and exchanges for access tokens
        async fn token_exchange_handler(
            State(app_state): State<AppState>,
            Json(body): Json<serde_json::Value>,
        ) -> impl IntoResponse {
            use crate::internal::auth::wimse::{Audience, IdentityValidator, TokenExchangeRequest};

            // Parse the request
            let req: TokenExchangeRequest = match serde_json::from_value(body) {
                Ok(r) => r,
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "invalid_request",
                            "message": format!("Failed to parse request: {}", e),
                        })),
                    );
                }
            };

            // Check if WIMSE is configured
            let wimse_config = match &app_state.server.config.wimse {
                Some(c) if c.enabled => c,
                _ => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({
                            "error": "wimse_disabled",
                            "message": "WIMSE token exchange is not enabled. Add [wimse] section to config.",
                        })),
                    );
                }
            };

            // Decode the signing key
            let signing_key = match base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &wimse_config.signing_key,
            ) {
                Ok(k) => k,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": "config_error",
                            "message": format!("Invalid signing_key (must be base64): {}", e),
                        })),
                    );
                }
            };

            // Build the validator
            let validator = IdentityValidator::new(&wimse_config.trust_domain, &signing_key);

            // Validate the platform JWT against the requested audience
            let expected_aud = Audience::from_str(&req.audience);

            if !wimse_config.allow_custom_audiences && matches!(expected_aud, Audience::Custom(_)) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_audience",
                        "message": format!(
                            "Custom audiences not allowed. Use weft:connector:<name>, weft:token-exchange, or weft:platform-internal. Got: {}",
                            req.audience
                        ),
                    })),
                );
            }

            let token = match validator.validate_for_audience(&req.platform_jwt, &expected_aud) {
                Ok(t) => t,
                Err(e) => {
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "error": "invalid_token",
                            "message": format!("Platform JWT validation failed: {}", e),
                        })),
                    );
                }
            };

            // Extract connector name from audience
            let connector = token.aud.connector_name().unwrap_or("unknown");

            // Look up access token from the connector cache
            let maybe_access_token = app_state.server.connector_tokens.get(connector);

            match maybe_access_token {
                Some(access_token) => {
                    tracing::info!(
                        agent = %token.sub,
                        connector = %connector,
                        sandbox = %token.sandbox_id,
                        depth = %token.delegation_chain.depth(),
                        "token exchange: WIMSE identity validated, access token returned"
                    );

                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "status": "ok",
                            "access_token": access_token,
                            "token_type": "Bearer",
                            "connector": connector,
                            "agent_id": token.sub,
                            "sandbox_id": token.sandbox_id,
                            "trust_domain": token.trust_domain,
                            "delegation_depth": token.delegation_chain.depth(),
                            "autonomous": token.delegation_chain.has_autonomous_action(),
                        })),
                    )
                }
                None => {
                    tracing::warn!(
                        agent = %token.sub,
                        connector = %connector,
                        "token exchange: identity valid but no access token cached for connector"
                    );

                    (
                        StatusCode::PRECONDITION_REQUIRED,
                        Json(serde_json::json!({
                            "status": "identity_validated",
                            "access_token": null,
                            "connector": connector,
                            "agent_id": token.sub,
                            "sandbox_id": token.sandbox_id,
                            "trust_domain": token.trust_domain,
                            "delegation_depth": token.delegation_chain.depth(),
                            "autonomous": token.delegation_chain.has_autonomous_action(),
                            "message": "WIMSE identity validated. No access token cached for this connector. Push a token via POST /api/auth/tokens first."
                        })),
                    )
                }
            }
        }

        // Token push handler — Weft pushes OAuth2 tokens into the connector cache
        async fn token_push_handler(
            State(app_state): State<AppState>,
            Json(body): Json<serde_json::Value>,
        ) -> impl IntoResponse {
            let connector = match body.get("connector").and_then(|v| v.as_str()) {
                Some(c) => c,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "missing_connector",
                            "message": "Request must include 'connector' field"
                        })),
                    );
                }
            };

            let access_token = match body.get("access_token").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "missing_access_token",
                            "message": "Request must include 'access_token' field"
                        })),
                    );
                }
            };

            let expires_in = body
                .get("expires_in")
                .and_then(|v| v.as_i64())
                .unwrap_or(3600);

            app_state
                .server
                .connector_tokens
                .store(connector, access_token, expires_in);

            tracing::info!(connector, expires_in, "connector token pushed to cache");

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "ok",
                    "connector": connector,
                    "cached_connectors": app_state.server.connector_tokens.len(),
                })),
            )
        }

        let mut app = axum::Router::new()
            .route("/health", get(health))
            .route("/metrics", get(metrics_handler))
            .route("/.well-known/ai-catalog.json", get(ai_catalog_handler))
            .route("/mcp", post(handle_mcp_request))
            .route("/api/auth/exchange", post(token_exchange_handler))
            .route("/api/auth/tokens", post(token_push_handler))
            .with_state(state);

        // Apply auth middleware if enabled
        if auth_enabled {
            let auth_mw = Arc::new(auth_middleware);
            app = app.layer(axum::middleware::from_fn(move |req, next| {
                let mw = Arc::clone(&auth_mw);
                async move { mw.handle(req, next).await }
            }));
            info!("Auth middleware layer applied");
        }

        // Merge A2A routes if enabled
        if let Some(a2a) = a2a_state {
            use crate::internal::a2a::router::{
                agent_card_handler, tasks_cancel_handler, tasks_get_handler, tasks_send_handler,
                tasks_send_subscribe_handler,
            };

            let a2a_routes = axum::Router::new()
                .route("/.well-known/agent-card.json", get(agent_card_handler))
                .route("/a2a/tasks/send", post(tasks_send_handler))
                .route(
                    "/a2a/tasks/sendSubscribe",
                    post(tasks_send_subscribe_handler),
                )
                .route("/a2a/tasks/get", get(tasks_get_handler))
                .route("/a2a/tasks/cancel", post(tasks_cancel_handler))
                .with_state(a2a);

            app = app.merge(a2a_routes);
            info!("A2A routes registered (5 endpoints)");
        }

        // 4. Start Server
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("Failed to bind to address: {}", addr))?;

        info!("HTTP MCP server listening on {}", addr);
        info!("Endpoint: POST http://{}/mcp", addr);
        info!(
            "AI Catalog: GET http://{}/.well-known/ai-catalog.json",
            addr
        );
        info!("Metrics: GET http://{}/metrics", addr);
        if wimse_enabled {
            info!(
                "WIMSE Token Exchange: POST http://{}/api/auth/exchange",
                addr
            );
            info!("WIMSE Token Push:    POST http://{}/api/auth/tokens", addr);
        }
        if a2a_enabled {
            info!(
                "A2A Agent Card: GET http://{}/.well-known/agent-card.json",
                addr
            );
            info!("A2A Tasks: POST http://{}/a2a/tasks/send", addr);
        }

        // Graceful shutdown: listen for SIGTERM/SIGINT, then drain
        let server = axum::serve(listener, app);
        let graceful = server.with_graceful_shutdown(async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C handler");
            info!("Shutdown signal received, draining connections...");
        });

        graceful.await.context("HTTP server failed")?;
        info!("Server shutdown complete");

        Ok(())
    }

    async fn list_tools_simple(&self) -> Result<ListToolsResult, McpError> {
        let tool_handler = self.tool_handler.lock().await;
        Ok(ListToolsResult {
            tools: tool_handler.list_tool_metadata(),
            ..Default::default()
        })
    }

    async fn call_tool_simple(
        &self,
        request: CallToolRequestParams,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.clone();
        let tool_handler = self.tool_handler.lock().await;

        if let Some(executor) = tool_handler.get_executor(&tool_name) {
            let executor = Arc::clone(&executor);
            drop(tool_handler);

            executor(request).await.map_err(|e| McpError {
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

    /// Serve in gRPC mode — experimental
    async fn serve_grpc(&self) -> Result<()> {
        use crate::internal::transport::{GrpcConfig, create_grpc_transport};

        let config = GrpcConfig {
            port: self.config.server.grpc_port,
            host: self.config.server.host.clone(),
            experimental_notice: true,
        };

        let tool_handler = self.tool_handler.lock().await;
        let registry = tool_handler.registry();
        drop(tool_handler);

        let transport = create_grpc_transport(config)?;
        transport.serve(self.tool_handler.clone(), registry).await
    }

    pub async fn start(&self) -> Result<()> {
        // Initialize metrics before anything that uses them
        crate::internal::telemetry::Metrics::init(&self.config.server.version);

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
            ServerMode::Grpc => self.serve_grpc().await,
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

    /// Build a SecretStore from config
    fn build_secret_store(&self) -> crate::internal::secrets::SecretStore {
        use crate::internal::secrets::SecretStore;

        let store = SecretStore::default(); // comes with env + file + literal

        // Register additional backends from config
        if let Some(secrets_cfg) = &self.config.secrets {
            for backend in &secrets_cfg.backends {
                match backend.backend_type.as_str() {
                    "vault" => {
                        tracing::info!(
                            "Vault secret backend configured. To enable, implement SecretResolver \
                             and register it before starting the server. See docs for example."
                        );
                    }
                    "aws-secretsmanager" => {
                        tracing::info!(
                            "AWS Secrets Manager backend configured. To enable, implement \
                             SecretResolver and register it before starting the server."
                        );
                    }
                    other => {
                        tracing::warn!(
                            "Unknown secret backend '{}'. Built-in backends: env, file, literal. \
                             Custom backends can be added via SecretStore::register().",
                            other
                        );
                    }
                }
            }
        }

        store
    }

    /// Build an AuthMiddleware from config, using the SecretStore to resolve secrets
    fn build_auth_middleware(
        &self,
        store: &crate::internal::secrets::SecretStore,
    ) -> crate::internal::auth::middleware::AuthMiddleware {
        use crate::internal::auth::middleware::AuthMiddleware;
        use crate::internal::auth::provider::AuthProvider;

        let auth_config = match &self.config.auth {
            Some(c) => c,
            None => return AuthMiddleware::passthrough(),
        };

        if auth_config.middleware_chain.is_empty() {
            return AuthMiddleware::passthrough();
        }

        let mut providers: Vec<Box<dyn AuthProvider>> = Vec::new();

        for provider_cfg in &auth_config.middleware_chain {
            let provider: Option<Box<dyn AuthProvider>> = match provider_cfg.provider_type.as_str()
            {
                "none" | "passthrough" => None,
                "bearer_token" | "bearer" => {
                    let token_raw = provider_cfg
                        .config
                        .get("token")
                        .cloned()
                        .unwrap_or_default();
                    let route = provider_cfg
                        .route_filter
                        .clone()
                        .unwrap_or_else(|| "/**".into());

                    let token = resolve_secret_sync(store, &token_raw);

                    Some(Box::new(BearerTokenProvider::new(&token, &route)))
                }
                "api_key" => {
                    let key_raw = provider_cfg.config.get("key").cloned().unwrap_or_default();
                    let header = provider_cfg
                        .config
                        .get("header")
                        .cloned()
                        .unwrap_or_else(|| "x-api-key".into());
                    let route = provider_cfg
                        .route_filter
                        .clone()
                        .unwrap_or_else(|| "/**".into());

                    let key = resolve_secret_sync(store, &key_raw);

                    Some(Box::new(ApiKeyProvider::new(&key, &header, &route)))
                }
                other => {
                    tracing::warn!("Unknown auth provider type: {}", other);
                    None
                }
            };

            if let Some(p) = provider {
                providers.push(p);
            }
        }

        if providers.is_empty() {
            AuthMiddleware::passthrough()
        } else {
            AuthMiddleware::new(providers, true)
        }
    }
}

// ── Helper: resolve a config value that may be a secret reference ──────────

/// Resolve a config value synchronously through the secret store.
/// If the value looks like a `scheme://...` reference, it's resolved.
/// Otherwise, it's returned as-is.
/// Falls back to the raw value if secret resolution fails.
fn resolve_secret_sync(store: &crate::internal::secrets::SecretStore, raw: &str) -> String {
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => match rt.block_on(store.resolve_value(raw)) {
            Ok(resolved) => resolved,
            Err(e) => {
                tracing::error!("Failed to resolve secret '{}': {}", raw, e);
                raw.to_string()
            }
        },
        Err(_) => {
            tracing::warn!("No tokio runtime available for secret resolution, using raw value");
            raw.to_string()
        }
    }
}

// ── Built-in auth providers ────────────────────────────────────────────────

/// Simple bearer token provider — checks Authorization: Bearer <token>
struct BearerTokenProvider {
    token: String,
    route_pattern: String,
}

impl BearerTokenProvider {
    fn new(token: &str, route_pattern: &str) -> Self {
        Self {
            token: token.to_string(),
            route_pattern: route_pattern.to_string(),
        }
    }
}

impl crate::internal::auth::provider::AuthProvider for BearerTokenProvider {
    fn provider_type(&self) -> &str {
        "bearer_token"
    }

    fn authenticate(
        &self,
        headers: &std::collections::HashMap<String, String>,
    ) -> anyhow::Result<Option<crate::internal::auth::provider::AuthIdentity>> {
        let auth_header = headers.get("authorization");
        match auth_header {
            Some(h) if h == &format!("Bearer {}", self.token) => {
                Ok(Some(crate::internal::auth::provider::AuthIdentity {
                    subject: "bearer-authenticated".into(),
                    name: None,
                    email: None,
                    provider: "bearer_token".into(),
                    claims: std::collections::HashMap::new(),
                }))
            }
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }

    fn matches_route(&self, path: &str) -> bool {
        // Simple prefix matching (glob-match integration in a future iteration)
        if self.route_pattern == "/**" {
            true
        } else {
            let pattern = self.route_pattern.trim_end_matches("/**");
            path.starts_with(pattern)
        }
    }
}

/// Simple API key provider — checks a configurable header for a static key
struct ApiKeyProvider {
    key: String,
    header_name: String,
    route_pattern: String,
}

impl ApiKeyProvider {
    fn new(key: &str, header_name: &str, route_pattern: &str) -> Self {
        Self {
            key: key.to_string(),
            header_name: header_name.to_lowercase(),
            route_pattern: route_pattern.to_string(),
        }
    }
}

impl crate::internal::auth::provider::AuthProvider for ApiKeyProvider {
    fn provider_type(&self) -> &str {
        "api_key"
    }

    fn authenticate(
        &self,
        headers: &std::collections::HashMap<String, String>,
    ) -> anyhow::Result<Option<crate::internal::auth::provider::AuthIdentity>> {
        let api_key = headers.get(&self.header_name);
        match api_key {
            Some(k) if k == &self.key => Ok(Some(crate::internal::auth::provider::AuthIdentity {
                subject: "api-key-authenticated".into(),
                name: None,
                email: None,
                provider: "api_key".into(),
                claims: std::collections::HashMap::new(),
            })),
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }

    fn matches_route(&self, path: &str) -> bool {
        if self.route_pattern == "/**" {
            true
        } else {
            let pattern = self.route_pattern.trim_end_matches("/**");
            path.starts_with(pattern)
        }
    }
}

pub async fn create_server(config: AppConfig) -> Result<Server> {
    let adjuster = Adjuster::new();
    let parser = Box::new(SwaggerParser::new(adjuster));
    let requester =
        HttpRequester::new(&config.endpoint).context("Failed to create HTTP requester")?;
    Server::new(config, parser, requester).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::auth::provider::AuthProvider;
    use crate::internal::config::EndpointConfig;
    use std::collections::HashMap;

    // ── BearerTokenProvider tests ──────────────────────────────────────

    #[test]
    fn test_bearer_provider_type() {
        let provider = BearerTokenProvider::new("secret", "/**");
        assert_eq!(provider.provider_type(), "bearer_token");
    }

    #[test]
    fn test_bearer_authenticate_valid() {
        let provider = BearerTokenProvider::new("my-token", "/**");
        let mut headers = HashMap::new();
        headers.insert("authorization".into(), "Bearer my-token".into());

        let result = provider.authenticate(&headers).unwrap();
        assert!(result.is_some());
        let identity = result.unwrap();
        assert_eq!(identity.subject, "bearer-authenticated");
        assert_eq!(identity.provider, "bearer_token");
    }

    #[test]
    fn test_bearer_authenticate_wrong_token() {
        let provider = BearerTokenProvider::new("my-token", "/**");
        let mut headers = HashMap::new();
        headers.insert("authorization".into(), "Bearer wrong".into());

        let result = provider.authenticate(&headers).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_bearer_authenticate_no_header() {
        let provider = BearerTokenProvider::new("my-token", "/**");
        let headers = HashMap::new();

        let result = provider.authenticate(&headers).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_bearer_matches_route_wildcard() {
        let provider = BearerTokenProvider::new("t", "/**");
        assert!(provider.matches_route("/anything"));
        assert!(provider.matches_route("/api/v1/users"));
        assert!(provider.matches_route("/"));
    }

    #[test]
    fn test_bearer_matches_route_prefix() {
        let provider = BearerTokenProvider::new("t", "/api/protected/**");
        assert!(provider.matches_route("/api/protected/users"));
        assert!(provider.matches_route("/api/protected"));
        assert!(!provider.matches_route("/api/public"));
        assert!(!provider.matches_route("/other"));
    }

    // ── ApiKeyProvider tests ───────────────────────────────────────────

    #[test]
    fn test_apikey_provider_type() {
        let provider = ApiKeyProvider::new("key123", "x-api-key", "/**");
        assert_eq!(provider.provider_type(), "api_key");
    }

    #[test]
    fn test_apikey_authenticate_valid() {
        let provider = ApiKeyProvider::new("key123", "x-api-key", "/**");
        let mut headers = HashMap::new();
        headers.insert("x-api-key".into(), "key123".into());

        let result = provider.authenticate(&headers).unwrap();
        assert!(result.is_some());
        let identity = result.unwrap();
        assert_eq!(identity.subject, "api-key-authenticated");
        assert_eq!(identity.provider, "api_key");
    }

    #[test]
    fn test_apikey_authenticate_wrong_key() {
        let provider = ApiKeyProvider::new("key123", "x-api-key", "/**");
        let mut headers = HashMap::new();
        headers.insert("x-api-key".into(), "wrong".into());

        let result = provider.authenticate(&headers).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_apikey_authenticate_custom_header() {
        let provider = ApiKeyProvider::new("secret", "x-custom-auth", "/**");
        let mut headers = HashMap::new();
        headers.insert("x-custom-auth".into(), "secret".into());

        let result = provider.authenticate(&headers).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_apikey_matches_route() {
        let provider = ApiKeyProvider::new("k", "x-api-key", "/admin/**");
        assert!(provider.matches_route("/admin/users"));
        assert!(provider.matches_route("/admin"));
        assert!(!provider.matches_route("/api/users"));
    }

    // ── AuthMiddleware builder test ─────────────────────────────────────

    #[test]
    fn test_build_auth_middleware_no_config() {
        let config = AppConfig::test_default();
        let server = Server {
            config,
            parser: Arc::new(tokio::sync::Mutex::new(Box::new(SwaggerParser::new(
                Adjuster::new(),
            )))),
            requester: HttpRequester::new(&EndpointConfig::default()).unwrap(),
            tool_handler: Arc::new(tokio::sync::Mutex::new(ToolHandler::new(
                false,
                Arc::new(ToolRegistry::new()),
            ))),
            route_configs: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            connector_tokens: ConnectorTokenCache::new(),
        };

        let store = crate::internal::secrets::SecretStore::default();
        let middleware = server.build_auth_middleware(&store);
        assert!(!middleware.is_enabled());
        assert_eq!(middleware.provider_count(), 0);
    }

    #[test]
    fn test_build_secret_store_default() {
        let config = AppConfig::test_default();
        let server = Server {
            config,
            parser: Arc::new(tokio::sync::Mutex::new(Box::new(SwaggerParser::new(
                Adjuster::new(),
            )))),
            requester: HttpRequester::new(&EndpointConfig::default()).unwrap(),
            tool_handler: Arc::new(tokio::sync::Mutex::new(ToolHandler::new(
                false,
                Arc::new(ToolRegistry::new()),
            ))),
            route_configs: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            connector_tokens: ConnectorTokenCache::new(),
        };

        let store = server.build_secret_store();
        assert!(store.has_scheme("env"));
        assert!(store.has_scheme("file"));
        assert!(store.has_scheme("literal"));
    }
}
