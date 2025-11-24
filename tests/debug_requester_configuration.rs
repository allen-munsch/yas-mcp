use reqwest::Client;
use serde_json::{json, Value};

/// Debug test to trace the HTTP requester configuration
#[tokio::test]
async fn debug_requester_configuration() {
    let client = Client::new();
    let mcp_url = "http://127.0.0.1:3000";
    
    println!("🔍 Debugging HTTP Requester Configuration");
    println!("=========================================");
    
    // Test 1: Check what happens with a simple tool call
    println!("\n1. Testing get_users_me tool...");
    
    let response: Value = client
        .post(&format!("{}/mcp", mcp_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
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
    
    println!("Full response: {}", serde_json::to_string_pretty(&response).unwrap());
    
    // Let's examine the error details more closely
    if let Some(error) = response.get("error") {
        println!("\n❌ Error details:");
        println!("  Code: {}", error.get("code").unwrap_or(&Value::Null));
        println!("  Message: {}", error.get("message").unwrap_or(&Value::Null));
    }
    
    if let Some(result) = response.get("result") {
        println!("\n✅ Result details:");
        if let Some(content) = result.get("content") {
            println!("  Content: {}", content);
        }
        if let Some(is_error) = result.get("isError") {
            println!("  IsError: {}", is_error);
        }
        if let Some(text) = result.get("content").and_then(|c| c.get(0)).and_then(|c| c.get("raw")).and_then(|r| r.get("text")) {
            println!("  Response text: {}", text);
        }
    }
    
    // Test 2: Let's also test a tool that should definitely fail if endpoint is wrong
    println!("\n2. Testing post_projects tool (should create data)...");
    
    let response2: Value = client
        .post(&format!("{}/mcp", mcp_url))
        .json(&json!({
            "jsonrpc": "2.0", 
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "post_projects",
                "arguments": {
                    "title": "Test Project",
                    "description": "Test project from debug",
                    "color": "#3B82F6"
                }
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    
    println!("Post response: {}", serde_json::to_string_pretty(&response2).unwrap());
}