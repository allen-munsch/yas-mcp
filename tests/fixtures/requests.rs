use serde_json::json;

/// Standard MCP initialize request
pub fn initialize_request(id: i32) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    })
}

/// Initialized notification (no id)
pub fn initialized_notification() -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
}

/// List tools request
pub fn list_tools_request(id: i32) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/list",
        "params": {}
    })
}

/// Call tool request
pub fn call_tool_request(
    id: i32,
    tool_name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    })
}

/// Ping request
// pub fn ping_request(id: i32) -> serde_json::Value {
//     json!({
//         "jsonrpc": "2.0",
//         "id": id,
//         "method": "ping"
//     })
// }

/// Malformed request (missing jsonrpc)
// pub fn malformed_request() -> serde_json::Value {
//     json!({
//         "id": 999,
//         "method": "initialize"
//     })
// }

/// Unknown method request
pub fn unknown_method_request(id: i32) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "unknown/method",
        "params": {}
    })
}
