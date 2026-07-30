use std::sync::Arc;

use crate::internal::{
    mcp::{
        protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpMethod},
        registry::ToolRegistry,
    },
    server::Server,
};
use rmcp::model::{CallToolRequestParams, ListToolsResult, ServerInfo};
use rmcp::ServerHandler;
use tracing; // Add tracing import

/// Pure MCP message processor - no I/O, just transforms
pub struct McpProcessor {
    pub(crate) server_info: ServerInfo,
    pub(crate) tool_registry: Arc<ToolRegistry>,
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
                    ..Default::default()
                };
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: Some(serde_json::to_value(result).unwrap()),
                    error: None,
                }
            }
            McpMethod::ToolsCall => {
                let params: Result<CallToolRequestParams, _> =
                    serde_json::from_value(request.params.clone().unwrap_or_default());

                if let Ok(params) = params {
                    if let Some(tool) = self.tool_registry.get(&params.name) {
                        match (tool.executor)(params).await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::mcp::protocol::{JsonRpcRequest, McpMethod};
    use crate::internal::mcp::registry::ToolRegistry;
    use std::sync::Arc;

    fn make_processor() -> (McpProcessor, Arc<ToolRegistry>) {
        let registry = Arc::new(ToolRegistry::new());
        // Create a minimal processor (ServerInfo is generated from a dummy)
        let mut server_info = rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder().enable_tools().build(),
        );
        server_info = server_info
            .with_server_info(rmcp::model::Implementation::new(
                "test-server",
                "1.0.0",
            ))
            .with_protocol_version(rmcp::model::ProtocolVersion::V_2026_07_28);
        server_info.instructions = Some("Test MCP Server".into());

        let processor = McpProcessor {
            server_info,
            tool_registry: registry.clone(),
        };
        (processor, registry)
    }

    #[tokio::test]
    async fn test_initialize() {
        let (processor, _) = make_processor();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "initialize".into(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            })),
        };

        let response = processor.process_request(&request).await;
        assert!(response.result.is_some());
        assert!(response.error.is_none());

        let result = response.result.unwrap();
        assert!(result.get("serverInfo").is_some());
        assert_eq!(
            result["serverInfo"]["name"],
            "test-server"
        );
        assert_eq!(
            result["protocolVersion"],
            "2026-07-28"
        );
    }

    #[tokio::test]
    async fn test_initialized_notification() {
        let (processor, _) = make_processor();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: None, // Notifications have no id
            method: "notifications/initialized".into(),
            params: None,
        };

        let response = processor.process_request(&request).await;
        // Notifications should have no result and no error
        assert!(response.result.is_none());
        assert!(response.error.is_none());
        assert!(response.id.is_none());
    }

    #[tokio::test]
    async fn test_tools_list_empty() {
        let (processor, _) = make_processor();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "tools/list".into(),
            params: Some(serde_json::json!({})),
        };

        let response = processor.process_request(&request).await;
        assert!(response.result.is_some());
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn test_tools_list_with_registered_tools() {
        let (processor, registry) = make_processor();

        // Register a tool
        use crate::internal::mcp::registry::RegisteredTool;
        use crate::internal::server::tool::handler::ToolExecutor;

        let tool = rmcp::model::Tool::new(
            "test_tool",
            "A test tool",
            Arc::new(serde_json::Map::new()),
        )
        .with_title("Test Tool");

        let executor: ToolExecutor = Arc::new(|_req| {
            Box::pin(async {
                Ok(rmcp::model::CallToolResult::success(vec![]))
            })
        });

        registry.register(
            "test_tool".into(),
            RegisteredTool {
                metadata: tool,
                executor,
            },
        );

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "tools/list".into(),
            params: Some(serde_json::json!({})),
        };

        let response = processor.process_request(&request).await;
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "test_tool");
    }

    #[tokio::test]
    async fn test_tools_call_nonexistent() {
        let (processor, _) = make_processor();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "tools/call".into(),
            params: Some(serde_json::json!({
                "name": "nonexistent",
                "arguments": {}
            })),
        };

        let response = processor.process_request(&request).await;
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32601);
    }

    #[tokio::test]
    async fn test_ping() {
        let (processor, _) = make_processor();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "ping".into(),
            params: None,
        };

        let response = processor.process_request(&request).await;
        assert!(response.result.is_some());
        assert_eq!(response.result.unwrap(), serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let (processor, _) = make_processor();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "some/unknown".into(),
            params: None,
        };

        let response = processor.process_request(&request).await;
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32601);
        assert!(error.message.contains("not found"));
    }

    #[test]
    fn test_parse_request_valid() {
        let input = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let request = McpProcessor::parse_request(input).unwrap();
        assert_eq!(request.method, "ping");
        assert_eq!(request.id, Some(serde_json::json!(1)));
    }

    #[test]
    fn test_parse_request_invalid_json() {
        let input = b"{ not json }";
        let result = McpProcessor::parse_request(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_response() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            result: Some(serde_json::json!({})),
            error: None,
        };
        let bytes = McpProcessor::serialize_response(&response);
        let parsed: JsonRpcResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.id, Some(serde_json::json!(1)));
        assert!(parsed.result.is_some());
    }

    #[test]
    fn test_mcp_method_from_str() {
        assert_eq!(McpMethod::from("initialize"), McpMethod::Initialize);
        assert_eq!(McpMethod::from("tools/list"), McpMethod::ToolsList);
        assert_eq!(McpMethod::from("tools/call"), McpMethod::ToolsCall);
        assert_eq!(McpMethod::from("ping"), McpMethod::Ping);
        assert_eq!(
            McpMethod::from("notifications/initialized"),
            McpMethod::Initialized
        );
        assert!(matches!(
            McpMethod::from("bogus"),
            McpMethod::Unknown(_)
        ));
    }
}
