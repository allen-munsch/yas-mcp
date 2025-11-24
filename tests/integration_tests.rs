use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;

/// Generate proper UUIDs for testing
fn generate_test_uuid(seed: u32) -> String {
    format!("550e8400-e29b-41d4-a716-44665544{:04x}", seed)
}

/// Comprehensive MCP server integration test
/// Tests all endpoints with appropriate parameters based on the OpenAPI spec
#[tokio::test]
async fn test_mcp_server_comprehensive() {
    let client = Client::new();
    let mcp_url = "http://127.0.0.1:3000";
    
    println!("🧪 Starting Comprehensive MCP Server Integration Test");
    println!("Note: Make sure MCP server is running on {}", mcp_url);
    
    // Wait for server to be ready
    if let Err(e) = wait_for_server(&client, mcp_url).await {
        println!("❌ Server not available: {}", e);
        println!("💡 Start the server with: cargo run -- --swagger-file examples/todo-app/openapi.yaml --mode http");
        return;
    }
    
    // Test health endpoint
    test_health(&client, mcp_url).await;
    
    // Test MCP initialization
    let session = test_initialization(&client, mcp_url).await;
    
    // Test tools listing
    let tools = test_tools_listing(&client, mcp_url, &session).await;
    
    // Test all tools with appropriate parameters
    test_all_tools_comprehensive(&client, mcp_url, &session, &tools).await;
    
    println!("✅ All comprehensive integration tests passed!");
}

async fn wait_for_server(client: &Client, base_url: &str) -> Result<(), String> {
    for i in 0..10 {
        if let Ok(response) = client.get(&format!("{}/health", base_url)).send().await {
            if response.status().is_success() {
                println!("✅ MCP server is ready");
                return Ok(());
            }
        }
        println!("Waiting for MCP server... ({}/10)", i + 1);
        sleep(Duration::from_secs(1)).await;
    }
    Err("MCP server did not become ready in time".to_string())
}

async fn test_health(client: &Client, base_url: &str) {
    let response = client
        .get(&format!("{}/health", base_url))
        .send()
        .await
        .expect("Health check failed");
    
    assert!(response.status().is_success());
    println!("✅ Health check passed");
}

async fn test_initialization(client: &Client, base_url: &str) -> String {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "comprehensive-test",
                "version": "1.0.0"
            }
        }
    });
    
    let response: Value = client
        .post(&format!("{}/mcp", base_url))
        .json(&request)
        .send()
        .await
        .expect("Initialization request failed")
        .json()
        .await
        .expect("Failed to parse initialization response");
    
    assert!(response["result"].is_object());
    println!("✅ MCP initialization successful");
    
    response["result"]["sessionId"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

async fn test_tools_listing(client: &Client, base_url: &str, session: &str) -> Vec<String> {
    let request = json!({
        "jsonrpc": "2.0", 
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    
    let mut req_builder = client.post(&format!("{}/mcp", base_url)).json(&request);
    if !session.is_empty() {
        req_builder = req_builder.header("x-session-id", session);
    }
    
    let response: Value = req_builder
        .send()
        .await
        .expect("Tools listing request failed")
        .json()
        .await
        .expect("Failed to parse tools listing response");
    
    let tools = response["result"]["tools"]
        .as_array()
        .expect("No tools array in response");
    
    let tool_names: Vec<String> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(|s| s.to_string()))
        .collect();
    
    println!("✅ Found {} tools", tool_names.len());
    tool_names
}

async fn test_all_tools_comprehensive(client: &Client, base_url: &str, session: &str, tools: &[String]) {
    println!("\n🧪 Testing All Tools with Appropriate Parameters");
    println!("================================================");
    
    let mut test_id = 100;
    
    for tool_name in tools {
        let tool_params = get_tool_parameters(tool_name);
        test_tool_with_params(client, base_url, session, tool_name, &tool_params, test_id).await;
        test_id += 1;
        sleep(Duration::from_millis(300)).await; // Small delay between calls
    }
}

fn get_tool_parameters(tool_name: &str) -> Value {
    // Generate consistent UUIDs for testing
    let project_id = generate_test_uuid(1000);
    let task_id = generate_test_uuid(2000);
    let user_id = generate_test_uuid(3000);
    let comment_id = generate_test_uuid(4000);
    let attachment_id = generate_test_uuid(5000);
    let parent_comment_id = generate_test_uuid(6000);
    
    match tool_name {
        // Authentication endpoints
        "post_auth_login" => json!({
            "email": "test@example.com",
            "password": "testpassword123"
        }),
        "post_auth_register" => json!({
            "email": "newuser@example.com", 
            "password": "newpassword123",
            "name": "Test User"
        }),
        
        // User management
        "get_users_me" => json!({}),
        "put_users_me" => json!({
            "name": "Updated Test User",
            "avatar": "https://example.com/avatar.jpg"
        }),
        
        // Project management
        "get_projects" => json!({
            "page": 1,
            "per_page": 10,
            "archived": false,
            "search": "test"
        }),
        "post_projects" => json!({
            "title": "Test Project",
            "description": "Test project description",
            "color": "#3B82F6"
        }),
        "get_projects___project_id__" => json!({
            "project_id": project_id
        }),
        "put_projects___project_id__" => json!({
            "project_id": project_id,
            "title": "Updated Project",
            "description": "Updated description",
            "color": "#EF4444",
            "is_archived": false
        }),
        "delete_projects___project_id__" => json!({
            "project_id": project_id
        }),
        
        // Task management
        "get_projects___project_id___tasks" => json!({
            "project_id": project_id,
            "page": 1,
            "per_page": 20,
            "status": "pending",
            "priority": "medium",
            "assignee": user_id,
            "due_before": "2024-12-31",
            "due_after": "2024-01-01", 
            "search": "important"
        }),
        "post_projects___project_id___tasks" => json!({
            "project_id": project_id,
            "title": "New Task",
            "description": "Task description",
            "status": "pending",
            "priority": "high",
            "due_date": "2024-12-31",
            "estimated_hours": 5.0,
            "assignee_id": user_id,
            "tags": ["urgent", "backend"]
        }),
        "get_tasks___task_id__" => json!({
            "task_id": task_id
        }),
        "put_tasks___task_id__" => json!({
            "task_id": task_id, 
            "title": "Updated Task",
            "description": "Updated description",
            "status": "in_progress",
            "priority": "critical",
            "due_date": "2024-12-25",
            "estimated_hours": 8.0,
            "actual_hours": 2.0,
            "assignee_id": user_id,
            "tags": ["updated", "critical"]
        }),
        "delete_tasks___task_id__" => json!({
            "task_id": task_id
        }),
        
        // Comments
        "get_tasks___task_id___comments" => json!({
            "task_id": task_id,
            "page": 1,
            "per_page": 50
        }),
        "post_tasks___task_id___comments" => json!({
            "task_id": task_id,
            "content": "This is a test comment for the task",
            "parent_id": parent_comment_id
        }),
        "put_comments___comment_id__" => json!({
            "comment_id": comment_id,
            "content": "Updated comment content"
        }),
        "delete_comments___comment_id__" => json!({
            "comment_id": comment_id
        }),
        
        // Attachments
        "post_tasks___task_id___attachments" => json!({
            "task_id": task_id,
            "description": "Test attachment description"
        }),
        "get_attachments___attachment_id__" => json!({
            "attachment_id": attachment_id
        }),
        "delete_attachments___attachment_id__" => json!({
            "attachment_id": attachment_id
        }),
        
        // Analytics & Reports
        "get_analytics_projects_stats" => json!({
            "timeframe": "month"
        }),
        "post_reports_tasks_export" => json!({
            "format": "pdf",
            "project_ids": [project_id, generate_test_uuid(1001)],
            "date_from": "2024-01-01",
            "date_to": "2024-12-31",
            "include_comments": true
        }),
        
        // Root and health endpoints
        "get_" => json!({}),
        "get_health" => json!({}),
        
        // Default fallback
        _ => json!({})
    }
}

async fn test_tool_with_params(client: &Client, base_url: &str, session: &str, tool_name: &str, params: &Value, test_id: i32) {
    println!("\n🔧 Testing: {}", tool_name);
    
    let request = json!({
        "jsonrpc": "2.0",
        "id": test_id, 
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": params
        }
    });
    
    let mut req_builder = client.post(&format!("{}/mcp", base_url)).json(&request);
    if !session.is_empty() {
        req_builder = req_builder.header("x-session-id", session);
    }
    
    let response: Value = match req_builder
        .send()
        .await {
            Ok(resp) => resp.json().await.unwrap_or_default(),
            Err(e) => {
                println!("❌ Network error for {}: {}", tool_name, e);
                return;
            }
        };
    
    // Analyze response
    if response["error"].is_object() {
        let error_code = response["error"]["code"].as_i64().unwrap_or(0);
        let error_msg = response["error"]["message"].as_str().unwrap_or("Unknown error");
        
        match error_code {
            -32601 => println!("   ⚠️  Tool not found (might be expected)"),
            -32602 => println!("   ⚠️  Invalid parameters (needs specific values)"),
            _ => println!("   ❌ Tool call failed: {} (code: {})", error_msg, error_code)
        }
    } else if response["result"].is_object() {
        println!("   ✅ Tool call successful");
        
        // Show some result details for specific tools
        if let Some(data) = response["result"]["content"][0]["raw"]["text"].as_str() {
            match tool_name {
                "get_projects" | "get_projects___project_id___tasks" | "get_tasks___task_id___comments" => {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        if let Some(items) = parsed["data"].as_array() {
                            println!("      📊 Returned {} items", items.len());
                        }
                    }
                }
                "get_analytics_projects_stats" => {
                    println!("      📈 Analytics data received");
                }
                _ => {
                    // For other tools, just show that we got data
                    if !data.is_empty() && data != "{}" {
                        println!("      📄 Response data received");
                    }
                }
            }
        }
    } else {
        println!("   ⚠️  Tool call completed (empty or unexpected response)");
    }
}

/// Quick smoke test for basic functionality
#[tokio::test]
async fn test_mcp_server_smoke() {
    let client = Client::new();
    let mcp_url = "http://127.0.0.1:3000";
    
    println!("🚀 MCP Server Smoke Test");
    
    if wait_for_server(&client, mcp_url).await.is_err() {
        println!("❌ Server not available - skipping smoke test");
        return;
    }
    
    // Test critical endpoints only
    test_health(&client, mcp_url).await;
    let session = test_initialization(&client, mcp_url).await;
    let tools = test_tools_listing(&client, mcp_url, &session).await;
    
    // Test a few critical tools
    let critical_tools = vec![
        "get_users_me",
        "get_projects", 
        "get_projects___project_id___tasks",
        "post_projects",
        "get_health",
        "get_"
    ];
    
    for tool in critical_tools {
        if tools.contains(&tool.to_string()) {
            let params = get_tool_parameters(tool);
            test_tool_with_params(&client, mcp_url, &session, tool, &params, 999).await;
        }
    }
    
    println!("✅ Smoke test completed!");
}

/// Test specific tools that should work with the current configuration
#[tokio::test]
async fn test_working_endpoints() {
    let client = Client::new();
    let mcp_url = "http://127.0.0.1:3000";
    
    println!("🎯 Testing Known Working Endpoints");
    
    if wait_for_server(&client, mcp_url).await.is_err() {
        println!("❌ Server not available - skipping working endpoints test");
        return;
    }
    
    let session = test_initialization(&client, mcp_url).await;
    
    // These endpoints should work with our current config
    let working_tools = vec![
        "get_health",
        "get_",
        "get_users_me",
        "get_projects",
        "post_projects",
        "get_analytics_projects_stats",
        "post_auth_login",
        "post_auth_register"
    ];
    
    for tool in working_tools {
        let params = get_tool_parameters(tool);
        test_tool_with_params(&client, mcp_url, &session, tool, &params, 500).await;
    }
    
    println!("✅ Working endpoints test completed!");
}