//! Debug test for HTTP requester configuration.
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

#[tokio::test]
async fn debug_requester_configuration() {
    let client = Client::new();
    let url = mcp_url();

    println!("🔍 Debugging HTTP Requester Configuration");
    println!("Server: {url}");

    if !server_available(&client, &url).await {
        println!("⏭️  Server not available, skipping debug test");
        return;
    }

    // List tools
    let raw = client
        .post(format!("{url}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await;

    let response: Value = match raw {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(e) => {
            println!("⚠️  tools/list request failed: {e}");
            return;
        }
    };

    let tools = response["result"]["tools"].as_array();
    if tools.is_none() {
        println!("⚠️  Unexpected response from server (maybe not yas-mcp?)");
        return;
    }

    if let Some(tools) = tools {
        if !tools.is_empty() {
            let first_tool = &tools[0];
            let name = first_tool["name"].as_str().unwrap_or("?");
            println!("Testing tool call: {name}");

            let call_response: Value = client
                .post(format!("{url}/mcp"))
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {"name": name, "arguments": {}}
                }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

            if call_response.get("error").is_some() {
                println!(
                    "Error: {}",
                    call_response["error"]["message"].as_str().unwrap_or("?")
                );
            } else {
                println!("✅ Tool call returned result");
            }
        }
    }

    println!("✅ Debug test complete");
}
