//! Debug tests for endpoint configuration.
//! Requires a running yas-mcp server.
//! Set MCP_SERVER_URL env var (default: http://127.0.0.1:3000).

use reqwest::Client;
use serde_json::{json, Value};

fn mcp_url() -> String {
    std::env::var("MCP_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into())
}

async fn server_available(client: &Client, url: &str) -> bool {
    if let Ok(response) = client.get(format!("{url}/health")).send().await {
        response.status().is_success()
    } else {
        false
    }
}

/// Debug test to check endpoint configuration
#[tokio::test]
async fn debug_endpoint_configuration() {
    let client = Client::new();
    let url = mcp_url();

    println!("🔍 Debugging Endpoint Configuration");
    println!("Server: {url}");

    if !server_available(&client, &url).await {
        println!("⏭️  Server not available, skipping debug test");
        return;
    }

    let raw_init = client
        .post(format!("{url}/mcp"))
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
        .await;

    let init_response: Value = match raw_init {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(e) => {
            println!("⚠️  Initialize request failed: {e}");
            return;
        }
    };

    if init_response.is_null() || !init_response["result"].is_object() {
        println!("⚠️  Server returned unexpected response (maybe not yas-mcp?)");
        return;
    }

    println!(
        "Server info: {}",
        serde_json::to_string_pretty(&init_response["result"]).unwrap_or_default()
    );

    // Try a tool call
    let raw_tools = client
        .post(format!("{url}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await;

    let tools_response: Value = match raw_tools {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => {
            println!("⚠️  tools/list request failed");
            return;
        }
    };

    let tool_count = tools_response["result"]["tools"]
        .as_array()
        .map(|t| t.len())
        .unwrap_or(0);
    println!("Tools available: {tool_count}");

    // Check A2A if available
    if let Ok(resp) = client
        .get(format!("{url}/.well-known/agent-card.json"))
        .send()
        .await
    {
        if resp.status().is_success() {
            let card: Value = resp.json().await.unwrap_or_default();
            let skills = card["skills"].as_array().map(|s| s.len()).unwrap_or(0);
            println!("A2A Agent Card available with {skills} skills");
        } else {
            println!("A2A not enabled (HTTP {})", resp.status());
        }
    }

    println!("✅ Debug test complete");
}
