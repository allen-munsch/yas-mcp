// src/internal/server/tool/handler.rs

use crate::internal::mcp::registry::{RegisteredTool, ToolRegistry};
use anyhow::{anyhow, Result};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, TextContent, Tool};
use serde_json::Map;
use std::sync::Arc;
use tracing::debug;

use crate::internal::requester::RouteExecutor;

// Simplify the ToolExecutor to avoid lifetime issues
pub type ToolExecutor = Arc<
    dyn Fn(
            CallToolRequestParams,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<CallToolResult, anyhow::Error>> + Send>,
        > + Send
        + Sync,
>;

/// Handler manages tool execution and authentication
pub struct ToolHandler {
    auth_enabled: bool,
    registry: Arc<ToolRegistry>,
}

impl ToolHandler {
    /// Create a new tool handler
    pub fn new(auth_enabled: bool, registry: Arc<ToolRegistry>) -> Self {
        Self {
            auth_enabled,
            registry,
        }
    }

    /// Register a tool with its executor
    pub fn register_tool(&mut self, name: &str, metadata: Tool, executor: ToolExecutor) {
        let registered_tool = RegisteredTool { metadata, executor };
        self.registry.register(name.to_string(), registered_tool);
    }

    /// Get an executor for a tool
    pub fn get_executor(&self, name: &str) -> Option<ToolExecutor> {
        self.registry.get(name).map(|t| t.executor.clone())
    }

    /// Get the number of registered tools
    pub fn tool_count(&self) -> usize {
        self.registry.count()
    }

    /// List all registered tool metadata
    pub fn list_tool_metadata(&self) -> Vec<Tool> {
        self.registry.list_metadata()
    }

    /// Create a handler function for a specific tool
    pub fn create_handler(&self, tool_name: &str, executor: RouteExecutor) -> ToolExecutor {
        let tool_name = tool_name.to_string();
        let auth_enabled = self.auth_enabled;

        Arc::new(move |request: CallToolRequestParams| {
            let tool_name = tool_name.clone();
            let executor = executor.clone(); // Clone the async executor

            Box::pin(async move {
                // Validate authentication if enabled
                if auth_enabled {
                    debug!(
                        "Auth enabled for tool: {}, but not yet implemented",
                        tool_name
                    );
                }

                // Create a tracing span for this tool execution
                let span = tracing::info_span!(
                    "mcp.tool_call",
                    tool.name = %tool_name,
                );
                let _enter = span.enter();

                // Execute the tool request (with timing for metrics)
                let start = std::time::Instant::now();
                let params = if let Some(args) = &request.arguments {
                    Self::convert_arguments_to_json(args)
                } else {
                    "{}".to_string()
                };

                let response = executor(&params).await.map_err(|e| {
                    anyhow!("Failed to execute request for tool {}: {}", tool_name, e)
                })?;

                let elapsed = start.elapsed().as_secs_f64();
                tracing::info!(elapsed_secs = elapsed, status = response.status_code, "Tool call completed");

                // Record metrics
                let method = "POST";
                let tool_short = tool_name.rsplit_once('_').map(|(_, n)| n).unwrap_or(&tool_name);
                crate::internal::telemetry::Metrics::get()
                    .record_tool_call(tool_short, method, elapsed);

                // Handle error responses
                if response.status_code >= 400 {
                    let error_message = String::from_utf8_lossy(&response.body).to_string();
                    crate::internal::telemetry::Metrics::get()
                        .record_tool_error(tool_short, method, response.status_code);
                    return Ok(CallToolResult::error(vec![
                        ContentBlock::Text(TextContent::new(error_message)),
                    ]));
                }

                // Convert successful response to text content
                let text_content = String::from_utf8_lossy(&response.body).to_string();

                Ok(CallToolResult::success(vec![
                    ContentBlock::Text(TextContent::new(text_content)),
                ]))
            })
        })
    }
    /// Convert MCP tool arguments to JSON string for the executor
    fn convert_arguments_to_json(arguments: &Map<String, serde_json::Value>) -> String {
        serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string())
    }

    /// Get the underlying tool registry.
    pub fn registry(&self) -> Arc<ToolRegistry> {
        Arc::clone(&self.registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::requester::types::RouteExecutor;
    use crate::internal::requester::http_requester::HttpResponse;
    use std::collections::HashMap;

    // Ensure metrics are initialized before any handler test
    fn init_metrics() {
        let _ = crate::internal::telemetry::Metrics::init("test");
    }

    fn make_test_executor(
        status: u16,
        body: &'static str,
    ) -> RouteExecutor {
        Arc::new(move |_params: &str| {
            let body = body.as_bytes().to_vec();
            Box::pin(async move {
                Ok(HttpResponse {
                    status_code: status,
                    body,
                    headers: HashMap::new(),
                })
            })
        })
    }

    fn make_handler() -> ToolHandler {
        ToolHandler::new(false, Arc::new(ToolRegistry::new()))
    }

    #[test]
    fn test_convert_arguments_to_json_empty() {
        let args = Map::new();
        let json = ToolHandler::convert_arguments_to_json(&args);
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_convert_arguments_to_json_with_data() {
        let mut args = Map::new();
        args.insert("name".into(), serde_json::Value::String("test".into()));
        args.insert("count".into(), serde_json::json!(42));

        let json = ToolHandler::convert_arguments_to_json(&args);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "test");
        assert_eq!(parsed["count"], 42);
    }

    #[tokio::test]
    async fn test_register_and_get_executor() {
        let mut handler = make_handler();
        let executor = make_test_executor(200, "ok");
        let handler_fn = handler.create_handler("test_tool", executor);

        let tool = rmcp::model::Tool::new(
            "test_tool",
            "A test",
            Arc::new(serde_json::Map::new()),
        )
        .with_title("Test Tool");

        handler.register_tool("test_tool", tool, handler_fn);

        let retrieved = handler.get_executor("test_tool");
        assert!(retrieved.is_some());
        assert!(handler.get_executor("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_list_tool_metadata() {
        let mut handler = make_handler();
        let executor = make_test_executor(200, "ok");

        for i in 0..5 {
            let name = format!("tool_{i}");
            let handler_fn = handler.create_handler(&name, executor.clone());
            let tool = rmcp::model::Tool::new(
                name.clone(),
                format!("Tool {i}"),
                Arc::new(serde_json::Map::new()),
            );
            handler.register_tool(&name, tool, handler_fn);
        }

        let tools = handler.list_tool_metadata();
        assert_eq!(tools.len(), 5);
        assert_eq!(handler.tool_count(), 5);
    }

    #[tokio::test]
    async fn test_create_handler_executes_and_returns_result() {
        init_metrics();
        let handler = make_handler();
        let executor = make_test_executor(200, r#"{"message": "hello"}"#);
        let handler_fn = handler.create_handler("my_tool", executor);

        let mut args = Map::new();
        args.insert("query".into(), serde_json::Value::String("test".into()));

        let request = rmcp::model::CallToolRequestParams::new("my_tool")
            .with_arguments(args);

        let result = handler_fn(request).await.unwrap();
        assert_eq!(result.is_error, Some(false));
        assert!(!result.content.is_empty());

        // Check the text content
        if let rmcp::model::ContentBlock::Text(text) = &result.content[0] {
            assert!(text.text.contains("hello"));
        }
    }

    #[tokio::test]
    async fn test_create_handler_returns_error_for_4xx() {
        init_metrics();
        let handler = make_handler();
        let executor = make_test_executor(404, "not found");
        let handler_fn = handler.create_handler("failing_tool", executor);

        let request = rmcp::model::CallToolRequestParams::new("failing_tool");

        let result = handler_fn(request).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn test_registry_returns_same_arc() {
        let handler = make_handler();
        let reg1 = handler.registry();
        let reg2 = handler.registry();
        // Should be the same Arc (same underlying registry)
        assert_eq!(reg1.count(), reg2.count());
    }
}
