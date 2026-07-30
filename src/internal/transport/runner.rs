use std::sync::Arc;

use crate::internal::{
    mcp::{
        processor::McpProcessor,
        protocol::{JsonRpcError, JsonRpcResponse},
    },
    transport::{Transport, TransportError},
};

pub struct TransportRunner<T: Transport> {
    transport: T,
    processor: Arc<McpProcessor>,
}

impl<T: Transport> TransportRunner<T> {
    pub fn new(transport: T, processor: Arc<McpProcessor>) -> Self {
        Self {
            transport,
            processor,
        }
    }

    pub async fn run(&mut self) -> Result<(), TransportError> {
        eprintln!("[TransportRunner] Starting run loop");
        loop {
            let input = match self.transport.read_message().await {
                Ok(data) => data,
                Err(TransportError::Closed) => {
                    eprintln!("[TransportRunner] Transport closed, exiting loop");
                    break;
                }
                Err(e) => {
                    eprintln!("[TransportRunner] Transport error: {:?}", e);
                    return Err(e);
                }
            };

            eprintln!(
                "[TransportRunner] Received {} bytes for processing",
                input.len()
            );

            // Parse request
            let request = match McpProcessor::parse_request(&input) {
                Ok(req) => {
                    eprintln!(
                        "[TransportRunner] Successfully parsed request: method={}",
                        req.method
                    );
                    req
                }
                Err(e) => {
                    eprintln!("[TransportRunner] Parse error: {}", e);
                    let error_response = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: None, // Parse errors usually don't have an ID
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {}", e),
                            data: None,
                        }),
                    };
                    let output = McpProcessor::serialize_response(&error_response);
                    self.transport.write_message(&output).await?;
                    self.transport.flush().await?;
                    continue;
                }
            };

            // Process request
            let response = self.processor.process_request(&request).await;
            eprintln!(
                "[TransportRunner] Processed request. Response: has_result={}, has_error={}",
                response.result.is_some(),
                response.error.is_some()
            );

            // Send response (skip for notifications)
            if request.id.is_some() {
                eprintln!("[TransportRunner] Writing response for id={:?}", request.id);
                let output = McpProcessor::serialize_response(&response);
                self.transport.write_message(&output).await?;
                self.transport.flush().await?;
            } else {
                eprintln!("[TransportRunner] Skipping response for notification (no ID)");
            }
        }

        eprintln!("[TransportRunner] Run loop finished successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::mcp::protocol::JsonRpcRequest;
    use crate::internal::mcp::registry::ToolRegistry;
    use crate::internal::transport::mock::MockTransport;

    fn make_processor() -> (McpProcessor, Arc<ToolRegistry>) {
        let registry = Arc::new(ToolRegistry::new());
        let mut server_info = rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        );
        server_info = server_info.with_server_info(rmcp::model::Implementation::new("test", "1.0"));
        server_info.instructions = Some("Test server".into());
        let processor = McpProcessor {
            server_info,
            tool_registry: registry.clone(),
        };
        (processor, registry)
    }

    #[tokio::test]
    async fn test_runner_processes_initialize() {
        let (processor, _) = make_processor();
        let transport = MockTransport::new();

        let request: JsonRpcRequest = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            }
        }))
        .unwrap();

        transport.queue_request(&request);

        let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
        let _ = runner.run().await;

        let responses = transport.get_responses();
        assert_eq!(responses.len(), 1);
        assert!(responses[0].result.is_some());
    }

    #[tokio::test]
    async fn test_runner_handles_notification() {
        let (processor, _) = make_processor();
        let transport = MockTransport::new();

        let notification: JsonRpcRequest = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .unwrap();

        transport.queue_request(&notification);

        let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
        let _ = runner.run().await;

        // No response for notifications
        let responses = transport.get_responses();
        assert_eq!(responses.len(), 0);
    }

    #[tokio::test]
    async fn test_runner_handles_malformed_input() {
        let (processor, _) = make_processor();
        let transport = MockTransport::new();

        transport.queue_input(b"{ not json }".to_vec());

        let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
        let _ = runner.run().await;

        let responses = transport.get_responses();
        assert_eq!(responses.len(), 1);
        assert!(responses[0].error.is_some());
        assert_eq!(responses[0].error.as_ref().unwrap().code, -32700);
    }

    #[tokio::test]
    async fn test_runner_handles_unknown_method() {
        let (processor, _) = make_processor();
        let transport = MockTransport::new();

        let request: JsonRpcRequest = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "unknown/method",
            "params": {}
        }))
        .unwrap();

        transport.queue_request(&request);

        let mut runner = TransportRunner::new(transport.clone(), Arc::new(processor));
        let _ = runner.run().await;

        let responses = transport.get_responses();
        assert_eq!(responses.len(), 1);
        assert!(responses[0].error.is_some());
        assert_eq!(responses[0].error.as_ref().unwrap().code, -32601);
    }
}
