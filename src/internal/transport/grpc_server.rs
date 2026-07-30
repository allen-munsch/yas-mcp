//! gRPC Server — MCP-over-gRPC Implementation
//!
//! Implements MCP-over-gRPC using `proto/mcp.proto` plus the standard
//! gRPC health checking protocol (grpc.health.v1).
//! When the MCP community standardizes gRPC, swap the proto file and rebuild.

use super::{GrpcConfig, McpTransport};
use crate::internal::mcp::registry::ToolRegistry;
use crate::internal::server::tool::ToolHandler;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::info;

// Generated protobuf code
pub mod mcp {
    pub mod v1 {
        tonic::include_proto!("mcp.v1");
    }
}

use mcp::v1::mcp_service_server::{McpService, McpServiceServer};
use mcp::v1::*;

/// gRPC server that implements the MCP service
pub struct GrpcServer {
    config: GrpcConfig,
}

impl GrpcServer {
    pub fn new(config: GrpcConfig) -> anyhow::Result<Self> {
        if config.experimental_notice {
            info!("gRPC transport — ready on port {}", config.port);
        }
        Ok(Self { config })
    }
}

/// The actual gRPC service implementation — bridges to the tool registry
struct McpServiceImpl {
    tool_handler: Arc<tokio::sync::Mutex<ToolHandler>>,
    #[allow(dead_code)]
    registry: Arc<ToolRegistry>,
}

#[tonic::async_trait]
impl McpService for McpServiceImpl {
    async fn initialize(
        &self,
        request: Request<InitializeRequest>,
    ) -> Result<Response<InitializeResponse>, Status> {
        let _req = request.into_inner();
        info!("[gRPC] initialize — client: {}", 
            _req.client_info.as_ref().map(|c| c.name.as_str()).unwrap_or("unknown"));

        Ok(Response::new(InitializeResponse {
            protocol_version: "2024-11-05".into(),
            capabilities: Some(ServerCapabilities {
                tools: true,
                streaming: true,
            }),
            server_info: Some(ServerInfo {
                name: "yas-mcp-grpc".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            }),
            instructions: "OpenAPI MCP Server via gRPC".into(),
        }))
    }

    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        let handler = self.tool_handler.lock().await;
        let tools = handler.list_tool_metadata();

        let proto_tools: Vec<Tool> = tools
            .iter()
            .map(|t| Tool {
                name: t.name.to_string(),
                title: t.title.clone().unwrap_or_default(),
                description: t.description.clone().unwrap_or_default().to_string(),
                input_schema: serde_json::to_string(&t.input_schema).unwrap_or_default(),
                output_schema: String::new(),
            })
            .collect();

        info!("[gRPC] list_tools — {} tools", proto_tools.len());

        Ok(Response::new(ListToolsResponse {
            tools: proto_tools,
            next_cursor: String::new(),
        }))
    }

    type CallToolStream = tokio_stream::wrappers::ReceiverStream<Result<CallToolResponse, Status>>;

    async fn call_tool(
        &self,
        request: Request<CallToolRequest>,
    ) -> Result<Response<Self::CallToolStream>, Status> {
        let req = request.into_inner();
        info!("[gRPC] call_tool — tool: {}", req.name);

        let handler = self.tool_handler.lock().await;
        let executor = handler
            .get_executor(&req.name)
            .ok_or_else(|| Status::not_found(format!("Tool '{}' not found", req.name)))?;
        drop(handler);

        // Parse arguments
        let arguments: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&req.arguments).unwrap_or_default();

        let mcp_request = rmcp::model::CallToolRequestParams::new(req.name.clone())
            .with_arguments(arguments);

        let tool_name = req.name.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        // Execute in background, stream results
        tokio::spawn(async move {
            match executor(mcp_request).await {
                Ok(result) => {
                    if result.is_error == Some(true) {
                        let error_text = result
                            .content
                            .first()
                            .map(|c| match c {
                                rmcp::model::ContentBlock::Text(t) => t.text.clone(),
                                _ => String::new(),
                            })
                            .unwrap_or_default();

                        let _ = tx
                            .send(Ok(CallToolResponse {
                                is_final: true,
                                content: Some(call_tool_response::Content::Text(
                                    TextContent { text: error_text },
                                )),
                                is_error: true,
                                error_message: "Tool execution failed".into(),
                            }))
                            .await;
                    } else {
                        for content in result.content {
                            let text = match content {
                                rmcp::model::ContentBlock::Text(t) => t.text.clone(),
                                _ => String::new(),
                            };

                            let _ = tx
                                .send(Ok(CallToolResponse {
                                    is_final: false,
                                    content: Some(call_tool_response::Content::Text(
                                        TextContent { text },
                                    )),
                                    is_error: false,
                                    error_message: String::new(),
                                }))
                                .await;
                        }

                        // Final message
                        let _ = tx
                            .send(Ok(CallToolResponse {
                                is_final: true,
                                content: None,
                                is_error: false,
                                error_message: String::new(),
                            }))
                            .await;
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(Ok(CallToolResponse {
                            is_final: true,
                            content: None,
                            is_error: true,
                            error_message: format!("Tool '{}' failed: {}", tool_name, e),
                        }))
                        .await;
                }
            }
        });

        Ok(Response::new(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }

    async fn ping(
        &self,
        _request: Request<PingRequest>,
    ) -> Result<Response<PingResponse>, Status> {
        Ok(Response::new(PingResponse {}))
    }
}

#[async_trait::async_trait]
impl McpTransport for GrpcServer {
    async fn serve(
        &self,
        tool_handler: Arc<tokio::sync::Mutex<ToolHandler>>,
        registry: Arc<ToolRegistry>,
    ) -> anyhow::Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid gRPC address: {}", e))?;

        let service = McpServiceImpl {
            tool_handler,
            registry,
        };

        info!(
            "gRPC server starting on {}",
            addr
        );

        let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
        health_reporter
            .set_serving::<McpServiceServer<McpServiceImpl>>()
            .await;

        tonic::transport::Server::builder()
            .add_service(health_service)
            .add_service(McpServiceServer::new(service))
            .serve_with_shutdown(addr, async {
                tokio::signal::ctrl_c().await.ok();
                info!("gRPC server shutting down");
            })
            .await?;

        Ok(())
    }

    fn name(&self) -> &str {
        "grpc"
    }

    fn is_experimental(&self) -> bool {
        false
    }
}
