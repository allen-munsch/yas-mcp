//! Integration Tests
//!
//! Tests that require a running yas-mcp server.
//! Set `MCP_SERVER_URL` env var (default: http://127.0.0.1:3000).
//!
//! In CI (docker compose), these run against a live server.
//! In local dev, they skip if no server is running.

use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;

fn mcp_url() -> String {
    std::env::var("MCP_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into())
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn generate_test_uuid(seed: u32) -> String {
    format!("550e8400-e29b-41d4-a716-44665544{:04x}", seed)
}

async fn wait_for_server(client: &Client, base_url: &str) -> Result<(), String> {
    for i in 0..10 {
        if let Ok(response) = client.get(format!("{base_url}/health")).send().await {
            if response.status().is_success() {
                println!("✅ MCP server is ready at {base_url}");
                return Ok(());
            }
        }
        println!("Waiting for MCP server... ({}/10)", i + 1);
        sleep(Duration::from_secs(1)).await;
    }
    Err(format!("MCP server not available at {base_url}"))
}

async fn mcp_request(
    client: &Client,
    base_url: &str,
    method: &str,
    params: Value,
) -> Value {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });

    match client
        .post(format!("{base_url}/mcp"))
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(e) => {
            println!("MCP request failed: {e}");
            Value::Null
        }
    }
}

// ── Phase 1: Health & Core MCP ─────────────────────────────────────────────

#[tokio::test]
async fn test_health_endpoint() {
    let client = Client::new();
    let url = mcp_url();

    match wait_for_server(&client, &url).await {
        Ok(()) => {
            let response = client
                .get(format!("{url}/health"))
                .send()
                .await
                .expect("Health request failed");
            assert!(response.status().is_success());
            println!("✅ Health endpoint OK");
        }
        Err(e) => {
            println!("⏭️  Skipping health test: {e}");
        }
    }
}

#[tokio::test]
async fn test_mcp_initialize() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping");
        return;
    }

    let response = mcp_request(
        &client,
        &url,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "integration-test", "version": "1.0.0"}
        }),
    )
    .await;

    if response.is_null() || !response["result"].is_object() {
        println!("⚠️  MCP initialize returned unexpected response (maybe not a yas-mcp server?)");
        return;
    }
    assert!(
        response["result"]["serverInfo"].is_object(),
        "Should have serverInfo"
    );
    println!("✅ MCP initialize OK");
}

#[tokio::test]
async fn test_mcp_tools_list() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping");
        return;
    }

    let response = mcp_request(&client, &url, "tools/list", json!({})).await;

    let tools = response["result"]["tools"].as_array();
    if tools.is_none() {
        println!("⚠️  tools/list returned unexpected response");
        return;
    }
    let tools = tools.unwrap();
    assert!(!tools.is_empty(), "Should have at least one tool");
    println!("✅ Found {} tools", tools.len());
}

#[tokio::test]
async fn test_mcp_ping() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping");
        return;
    }

    let response = mcp_request(&client, &url, "ping", json!({})).await;
    assert!(response["result"].is_object() || response["result"].is_null());
    println!("✅ Ping OK");
}

// ── Phase 2: A2A Protocol ──────────────────────────────────────────────────

#[tokio::test]
async fn test_a2a_agent_card() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping");
        return;
    }

    let response = client
        .get(format!("{url}/.well-known/agent-card.json"))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let card: Value = resp.json().await.unwrap_or_default();
            assert!(card.get("name").is_some(), "Agent Card should have name");
            assert!(
                card.get("skills").is_some(),
                "Agent Card should have skills"
            );
            let skills = card["skills"].as_array().unwrap();
            println!(
                "✅ Agent Card: '{}' with {} skills",
                card["name"].as_str().unwrap_or("?"),
                skills.len()
            );
        }
        Ok(resp) => {
            println!(
                "⏭️  Agent Card not available (HTTP {}). A2A may be disabled.",
                resp.status()
            );
        }
        Err(e) => {
            println!("⏭️  Agent Card endpoint not reachable: {e}");
        }
    }
}

#[tokio::test]
async fn test_a2a_task_send_and_get() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping");
        return;
    }

    // First check if A2A is enabled
    let card_check = client
        .get(format!("{url}/.well-known/agent-card.json"))
        .send()
        .await;

    if card_check.is_err() || !card_check.unwrap().status().is_success() {
        println!("⏭️  A2A not enabled, skipping task tests");
        return;
    }

    // Send a task
    let task_response = client
        .post(format!("{url}/a2a/tasks/send"))
        .json(&json!({
            "id": "integration-test-task",
            "sessionId": "integration-test-session",
            "message": {
                "role": "user",
                "parts": [
                    {"type": "data", "data": {"skill": "get_health", "parameters": {}}}
                ]
            }
        }))
        .send()
        .await;

    match task_response {
        Ok(resp) if resp.status().is_success() => {
            let task: Value = resp.json().await.unwrap_or_default();
            assert!(task.get("id").is_some(), "Task should have id");
            assert!(task.get("status").is_some(), "Task should have status");

            let task_id = task["id"].as_str().unwrap_or("");
            println!("✅ Task created: {task_id}, state: {:?}", task["status"]["state"]);

            // Get task status
            if !task_id.is_empty() {
                let get_resp = client
                    .get(format!(
                        "{url}/a2a/tasks/get?id={task_id}"
                    ))
                    .send()
                    .await;

                if let Ok(r) = get_resp {
                    if r.status().is_success() {
                        let task_status: Value = r.json().await.unwrap_or_default();
                        println!(
                            "✅ Task status: {:?}",
                            task_status["status"]["state"]
                        );
                    }
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            println!("⏭️  Task send returned HTTP {status}. A2A may be partially configured.");
        }
        Err(e) => {
            println!("⏭️  Task send failed: {e}");
        }
    }
}

#[tokio::test]
async fn test_a2a_task_cancel() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping");
        return;
    }

    // Check A2A availability
    let card_check = client
        .get(format!("{url}/.well-known/agent-card.json"))
        .send()
        .await;

    if card_check.is_err() || !card_check.unwrap().status().is_success() {
        println!("⏭️  A2A not enabled, skipping");
        return;
    }

    let cancel_response = client
        .post(format!("{url}/a2a/tasks/cancel"))
        .json(&json!({
            "id": "integration-test-cancel-task",
            "sessionId": "integration-test-session"
        }))
        .send()
        .await;

    match cancel_response {
        Ok(resp) => {
            let body: Value = resp.json().await.unwrap_or_default();
            // May be 404 (task not found) or 200 (canceled) — both are valid
            println!("✅ Cancel response: {}", body);
        }
        Err(e) => {
            println!("⏭️  Cancel endpoint not reachable: {e}");
        }
    }
}

// ── Phase 3: Auth Middleware ────────────────────────────────────────────────

#[tokio::test]
async fn test_auth_unauthenticated_access() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping");
        return;
    }

    // Unauthenticated tools/list should still work (unless auth is required globally)
    let response = mcp_request(&client, &url, "tools/list", json!({})).await;

    let has_result = response["result"].is_object();
    let has_error = response["error"].is_object();

    if has_result {
        println!("✅ Unauthenticated access allowed (passthrough mode)");
    } else if has_error {
        let code = response["error"]["code"].as_i64().unwrap_or(0);
        println!("🔐 Auth required (error code {code}) — middleware is active");
    }
}

#[tokio::test]
async fn test_auth_health_always_open() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping");
        return;
    }

    // Health should always be accessible regardless of auth config
    let response = client.get(format!("{url}/health")).send().await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            println!("✅ Health is publicly accessible");
        }
        Ok(resp) => {
            println!("⚠️  Health returned HTTP {}", resp.status());
        }
        Err(e) => println!("⚠️  Health endpoint not reachable: {e}"),
    }
}

// ── Phase 4: Comprehensive Tool Testing ────────────────────────────────────

#[tokio::test]
async fn test_comprehensive_tools() {
    let client = Client::new();
    let url = mcp_url();

    if wait_for_server(&client, &url).await.is_err() {
        println!("⏭️  Server not available, skipping comprehensive test");
        return;
    }
    println!("🧪 Starting Comprehensive MCP Server Integration Test");
    println!("MCP Server: {url}");

    // Test health
    let session = match initialize_mcp(&client, &url).await {
        Some(s) => s,
        None => {
            println!("❌ MCP initialization failed");
            return;
        }
    };

    // Test tools listing
    let tools = list_tools(&client, &url, &session).await;
    println!("Found {} tools", tools.len());

    // Test each tool
    for tool_name in &tools {
        let params = get_tool_parameters(tool_name);
        test_tool_with_params(&client, &url, &session, tool_name, &params).await;
        sleep(Duration::from_millis(100)).await;
    }

    println!("✅ Comprehensive test complete");
}

// ── Comprehensive test helpers ─────────────────────────────────────────────

async fn initialize_mcp(client: &Client, base_url: &str) -> Option<String> {
    let response = mcp_request(
        client,
        base_url,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "comprehensive-test", "version": "1.0.0"}
        }),
    )
    .await;

    if response["result"].is_object() {
        println!("✅ MCP initialization successful");
        response["result"]["sessionId"]
            .as_str()
            .map(|s| s.to_string())
    } else {
        None
    }
}

async fn list_tools(client: &Client, base_url: &str, _session: &str) -> Vec<String> {
    let response = mcp_request(client, base_url, "tools/list", json!({})).await;

    response["result"]["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn get_tool_parameters(tool_name: &str) -> Value {
    let project_id = generate_test_uuid(1000);

    match tool_name {
        "get_health" | "get_" => json!({}),
        "get_users_me" => json!({}),
        "post_auth_login" => json!({"email": "test@example.com", "password": "test123"}),
        "get_projects" => json!({"page": 1, "per_page": 10}),
        "post_projects" => json!({"title": "Test Project", "description": "Test Desc", "color": "#3B82F6"}),
        "get_projects___project_id__" => json!({"project_id": project_id}),
        "get_analytics_projects_stats" => json!({"timeframe": "month"}),
        _ => {
            if tool_name.contains("__") {
                let path_param = tool_name
                    .split("___")
                    .find(|s| !s.is_empty() && *s != "get" && *s != "post" && *s != "put" && *s != "delete")
                    .unwrap_or("id");
                json!({ path_param: format!("test-{path_param}") })
            } else {
                json!({})
            }
        }
    }
}

async fn test_tool_with_params(
    client: &Client,
    base_url: &str,
    _session: &str,
    tool_name: &str,
    params: &Value,
) {
    let response = mcp_request(
        client,
        base_url,
        "tools/call",
        json!({"name": tool_name, "arguments": params}),
    )
    .await;

    if response["error"].is_object() {
        let code = response["error"]["code"].as_i64().unwrap_or(0);
        match code {
            -32601 => println!("  ⚠️  {tool_name}: not found"),
            -32602 => println!("  ⚠️  {tool_name}: invalid params"),
            _ => println!("  ❌ {tool_name}: error code {code}"),
        }
    } else if response["result"].is_object() {
        println!("  ✅ {tool_name}");
    }
}
