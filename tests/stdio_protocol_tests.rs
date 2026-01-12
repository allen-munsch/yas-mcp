//! STDIO protocol tests - tests MCP protocol without real I/O

use std::sync::Arc;
use yas_mcp::internal::config::AppConfig;
use yas_mcp::internal::mcp::processor::McpProcessor;
use yas_mcp::internal::mcp::protocol::JsonRpcRequest;
use yas_mcp::internal::mcp::registry::ToolRegistry;
use yas_mcp::internal::server::_server::create_server;
use yas_mcp::internal::transport::mock::MockTransport;
use yas_mcp::internal::transport::runner::TransportRunner;

mod fixtures;

/// Test: Initialize handshake
#[tokio::test]
async fn test_initialize_handshake() {
    eprintln!("[TEST] Starting test_initialize_handshake");
    
    let (processor, _registry) = create_test_processor().await;
    eprintln!("[TEST] Created processor");
    
    let transport = MockTransport::new();
    eprintln!("[TEST] Created transport");

    let request: JsonRpcRequest = serde_json::from_value(
        fixtures::requests::initialize_request(1)
    ).unwrap();
    eprintln!("[TEST] Parsed initialize request: method={}", request.method);
    
    transport.queue_request(&request);
    eprintln!("[TEST] Queued initialize request. Original transport input queue size: {}", transport.inputs.lock().unwrap().len());

    let notification: JsonRpcRequest = serde_json::from_value(
        fixtures::requests::initialized_notification()
    ).unwrap();
    eprintln!("[TEST] Parsed initialized notification: method={}", notification.method);

    transport.queue_request(&notification);
    eprintln!("[TEST] Queued initialized notification. Original transport input queue size: {}", transport.inputs.lock().unwrap().len());


    let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
    eprintln!("[TEST] Created runner with cloned transport. Original transport input queue size before run(): {}", transport.inputs.lock().unwrap().len());
    
    let result = runner.run().await;
    eprintln!("[TEST] run() completed with result: {:?}", result);

    let responses = transport.get_responses();
    eprintln!("[TEST] Original transport got {} responses after run()", responses.len());
    
    assert_eq!(responses.len(), 1, "Expected 1 response (initialize only, not notification)");

    let init_response = &responses[0];
    assert!(init_response.result.is_some(), "Initialize should have result");
    assert!(init_response.error.is_none(), "Initialize should not have error");

    // Check server info in response
    let result = init_response.result.as_ref().unwrap();
    assert!(result.get("serverInfo").is_some(), "Should have serverInfo");
    assert!(result.get("protocolVersion").is_some(), "Should have protocolVersion");
    assert!(result.get("capabilities").is_some(), "Should have capabilities");
    eprintln!("[TEST] test_initialize_handshake finished successfully");
}

/// Test: List tools returns all registered tools
#[tokio::test]
async fn test_list_tools() {
    let (processor, registry) = create_test_processor().await;
    let transport = MockTransport::new();

    // Queue list tools request
    transport.queue_request(
        &serde_json::from_value(fixtures::requests::list_tools_request(1)).unwrap(),
    );

    let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
    let _ = runner.run().await;

    let responses = transport.get_responses();
    assert_eq!(responses.len(), 1, "Expected 1 response");

    let result = responses[0].result.as_ref().expect("Should have result");
    let tools = result.get("tools").expect("Should have tools").as_array().expect("tools should be array");

    // Should match registry count
    assert_eq!(tools.len(), registry.count(), "Tool count mismatch");
}

/// Test: Call tool with valid arguments
#[tokio::test]
async fn test_call_tool_valid() {
    let (processor, registry) = create_test_processor().await;
    let transport = MockTransport::new();

    // Get a real tool name from the registry
    let tools = registry.list_metadata();
    if tools.is_empty() {
        println!("No tools registered, skipping test");
        return;
    }
    let tool_name = tools[0].name.as_ref();

    transport.queue_request(
        &serde_json::from_value(fixtures::requests::call_tool_request(
            1,
            tool_name,
            serde_json::json!({}),
        ))
        .unwrap(),
    );

    let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
    let _ = runner.run().await;

    let responses = transport.get_responses();
    assert_eq!(responses.len(), 1, "Expected 1 response");

    // Should have result (may be error from backend, but not protocol error)
    assert!(responses[0].result.is_some() || responses[0].error.is_some(), "Should have result or error");
}

/// Test: Call unknown tool returns error
#[tokio::test]
async fn test_call_tool_unknown() {
    let (processor, _) = create_test_processor().await;
    let transport = MockTransport::new();

    transport.queue_request(
        &serde_json::from_value(fixtures::requests::call_tool_request(
            1,
            "nonexistent_tool_that_does_not_exist",
            serde_json::json!({}),
        ))
        .unwrap(),
    );

    let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
    let _ = runner.run().await;

    let responses = transport.get_responses();
    assert_eq!(responses.len(), 1, "Expected 1 response");

    // Should have error
    let error = responses[0].error.as_ref().expect("Should have error");
    assert_eq!(error.code, -32601, "Should be method not found error");
}

/// Test: Malformed JSON returns parse error
#[tokio::test]
async fn test_malformed_json() {
    let (processor, _) = create_test_processor().await;
    let transport = MockTransport::new();

    // Queue invalid JSON
    transport.queue_input(b"{ not valid json }".to_vec());

    let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
    let _ = runner.run().await;

    let responses = transport.get_responses();
    assert_eq!(responses.len(), 1, "Expected 1 response");

    let error = responses[0].error.as_ref().expect("Should have error");
    assert_eq!(error.code, -32700, "Should be parse error");
}

/// Test: Unknown method returns method not found
#[tokio::test]
async fn test_unknown_method() {
    let (processor, _) = create_test_processor().await;
    let transport = MockTransport::new();

    transport.queue_request(
        &serde_json::from_value(fixtures::requests::unknown_method_request(1)).unwrap(),
    );

    let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
    let _ = runner.run().await;

    let responses = transport.get_responses();
    assert_eq!(responses.len(), 1, "Expected 1 response");
    
    let error = responses[0].error.as_ref().expect("Should have error");
    assert_eq!(error.code, -32601, "Should be method not found");
}

/// Test: Notification does not receive response
#[tokio::test]
async fn test_notification_no_response() {
    let (processor, _) = create_test_processor().await;
    let transport = MockTransport::new();

    // Queue notification (no id)
    transport.queue_request(
        &serde_json::from_value(fixtures::requests::initialized_notification()).unwrap(),
    );

    let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
    let _ = runner.run().await;

    // No responses for notifications
    let responses = transport.get_responses();
    assert_eq!(responses.len(), 0, "Notifications should not get responses");
}

// Helper to create test processor with tools loaded
async fn create_test_processor() -> (McpProcessor, Arc<ToolRegistry>) {
    let config = AppConfig {
        swagger_file: "examples/todo-app/openapi.yaml".to_string(),
        ..Default::default()
    };

    let server = create_server(config).await.expect("Failed to create server");
    server.setup_tools().await.expect("Failed to setup tools");

    let tool_handler_guard = server.tool_handler.lock().await;
    let registry = tool_handler_guard.registry();

    (McpProcessor::new(&server, registry.clone()), registry)
}