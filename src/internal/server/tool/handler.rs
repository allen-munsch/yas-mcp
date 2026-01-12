// src/internal/server/tool/handler.rs

use crate::internal::mcp::registry::{RegisteredTool, ToolRegistry};
use anyhow::{anyhow, Result};
use rmcp::model::{Annotated, CallToolRequest, CallToolResult, RawContent, RawTextContent, Tool};
use serde_json::Map;
use std::sync::Arc;
use tracing::debug;

use crate::internal::requester::RouteExecutor;

// Simplify the ToolExecutor to avoid lifetime issues
pub type ToolExecutor = Arc<
    dyn Fn(
            CallToolRequest,
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

        Arc::new(move |request: CallToolRequest| {
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

                // Execute the tool request
                let params = if let Some(args) = &request.params.arguments {
                    Self::convert_arguments_to_json(args)
                } else {
                    "{}".to_string()
                };

                // Now executor is async, so we can await it directly
                let response = executor(&params).await.map_err(|e| {
                    anyhow!("Failed to execute request for tool {}: {}", tool_name, e)
                })?;

                // Handle error responses
                if response.status_code >= 400 {
                    let error_message = String::from_utf8_lossy(&response.body).to_string();
                    return Ok(CallToolResult {
                        content: vec![Annotated {
                            annotations: None,
                            raw: RawContent::Text(RawTextContent {
                                text: error_message,
                                meta: None,
                            }),
                        }],
                        is_error: Some(true),
                        meta: None,
                        structured_content: None,
                    });
                }

                // Convert successful response to text content
                let text_content = String::from_utf8_lossy(&response.body).to_string();

                let content = Annotated {
                    annotations: None,
                    raw: RawContent::Text(RawTextContent {
                        text: text_content,
                        meta: None,
                    }),
                };

                Ok(CallToolResult {
                    content: vec![content],
                    is_error: Some(false),
                    meta: None,
                    structured_content: None,
                })
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

