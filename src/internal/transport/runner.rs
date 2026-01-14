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
