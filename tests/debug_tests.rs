use reqwest::Client;
use serde_json::{json, Value};

/// Debug test to check endpoint configuration
#[tokio::test] 
async fn debug_endpoint_configuration() {
    let client = Client::new();
    let mcp_url = "http://127.0.0.1:3000";
    
    println!("🔍 Debugging Endpoint Configuration");
    println!("===================================");
    
    // Test 1: Check server info to see configured endpoint
    println!("\n1. Checking server configuration...");
    
    let init_response: Value = client
        .post(&format!("{}/mcp", mcp_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "debug-test", "version": "1.0.0"}
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    
    println!("Server info: {:?}", init_response["result"]["server_info"]);
    
    // Test 2: Make a tool call and see what happens
    println!("\n2. Testing tool call routing...");
    
    let tool_response: Value = client
        .post(&format!("{}/mcp", mcp_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "get_users_me",
                "arguments": {}
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    
    println!("Tool response: {}", serde_json::to_string_pretty(&tool_response).unwrap());
    
    // Test 3: Check if we can reach Prism directly
    println!("\n3. Testing direct Prism connection...");
    
    match client.get("http://127.0.0.1:4010/users/me").send().await {
        Ok(resp) => {
            println!("✅ Prism is reachable at localhost:4010");
            println!("   Status: {}", resp.status());
        }
        Err(e) => {
            println!("❌ Cannot reach Prism at localhost:4010: {}", e);
        }
    }
    
    // Test 4: Check what URL the MCP server is trying to call
    println!("\n4. Checking MCP server logs for endpoint calls...");
    println!("   (Look at your MCP server terminal for 'Executing request' logs)");
}