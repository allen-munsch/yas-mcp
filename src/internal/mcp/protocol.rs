use serde::{Deserialize, Serialize};

/// Raw JSON-RPC request envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// Raw JSON-RPC response envelope  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// MCP-specific method types
#[derive(Debug, Clone, PartialEq)]
pub enum McpMethod {
    Initialize,
    Initialized, // notification
    ToolsList,
    ToolsCall,
    Ping,
    Unknown(String),
}

impl From<&str> for McpMethod {
    fn from(s: &str) -> Self {
        match s {
            "initialize" => McpMethod::Initialize,
            "notifications/initialized" => McpMethod::Initialized,
            "tools/list" => McpMethod::ToolsList,
            "tools/call" => McpMethod::ToolsCall,
            "ping" => McpMethod::Ping,
            other => McpMethod::Unknown(other.to_string()),
        }
    }
}
