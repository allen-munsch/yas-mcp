use std::sync::Arc;

use crate::internal::{
    mcp::{
        protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpMethod},
        registry::ToolRegistry,
    },
    server::_server::Server,
};
use rmcp::model::{CallToolRequestParam, ListToolsResult, ServerInfo};
use rmcp::ServerHandler;
use tracing; // Add tracing import

/// Pure MCP message processor - no I/O, just transforms
pub struct McpProcessor {
    server_info: ServerInfo,
    tool_registry: Arc<ToolRegistry>,
}

impl McpProcessor {
    pub fn new(server: &Server, tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            server_info: server.get_info(),
            tool_registry,
        }
    }

    /// Process a raw JSON-RPC request and return a response
    /// This is the CORE testable unit
    pub async fn process_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let mcp_method = McpMethod::from(request.method.as_str());
        tracing::debug!("Processing request for method: {:?}", mcp_method);

        match mcp_method {
            McpMethod::Initialize => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: Some(serde_json::to_value(&self.server_info).unwrap()),
                error: None,
            },
            McpMethod::Initialized => {
                // No response for notifications
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: None,
                    error: None,
                }
            }
            McpMethod::ToolsList => {
                let tools = self.tool_registry.list_metadata();
                tracing::debug!("Tools listed: {:?}", tools); // Add debug print
                let result = ListToolsResult {
                    tools,
                    next_cursor: None,
                    meta: None,
                };
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: Some(serde_json::to_value(result).unwrap()),
                    error: None,
                }
            }
            McpMethod::ToolsCall => {
                let params: Result<CallToolRequestParam, _> =
                    serde_json::from_value(request.params.clone().unwrap_or_default());

                if let Ok(params) = params {
                    if let Some(tool) = self.tool_registry.get(&params.name) {
                        let call_request = rmcp::model::CallToolRequest {
                            method: rmcp::model::CallToolRequestMethod,
                            params,
                            extensions: Default::default(),
                        };
                        match (tool.executor)(call_request).await {
                            Ok(result) => JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: request.id.clone(),
                                result: Some(serde_json::to_value(result).unwrap()),
                                error: None,
                            },
                            Err(e) => JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: request.id.clone(),
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32000,
                                    message: e.to_string(),
                                    data: None,
                                }),
                            },
                        }
                    } else {
                        JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request.id.clone(),
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32601,
                                message: "Tool not found".to_string(),
                                data: None,
                            }),
                        }
                    }
                } else {
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id.clone(),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Invalid params".to_string(),
                            data: None,
                        }),
                    }
                }
            }
            McpMethod::Ping => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: Some(serde_json::json!({})),
                error: None,
            },
            McpMethod::Unknown(_) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: "Method not found".to_string(),
                    data: None,
                }),
            },
        }
    }

    /// Parse raw bytes into a request (handles line-delimited JSON)
    pub fn parse_request(input: &[u8]) -> Result<JsonRpcRequest, serde_json::Error> {
        serde_json::from_slice(input)
    }

    /// Serialize response to bytes
    pub fn serialize_response(response: &JsonRpcResponse) -> Vec<u8> {
        serde_json::to_vec(response).unwrap_or_default()
    }
}
