use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::internal::mcp::protocol::{JsonRpcRequest, JsonRpcResponse};

use super::{Transport, TransportError};

/// Mock transport for testing - allows injecting requests and capturing responses
#[derive(Clone)]
pub struct MockTransport {
    /// Queued inputs to be "read"
    pub inputs: Arc<Mutex<VecDeque<Vec<u8>>>>,
    /// Captured outputs
    pub outputs: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self {
            inputs: Arc::new(Mutex::new(VecDeque::new())),
            outputs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Queue a message to be read
    pub fn queue_input(&self, data: Vec<u8>) {
        self.inputs.lock().unwrap().push_back(data);
    }

    /// Queue a JSON-RPC request
    pub fn queue_request(&self, request: &JsonRpcRequest) {
        let json = serde_json::to_vec(request).unwrap();
        self.queue_input(json);
    }

    /// Get all captured outputs
    pub fn get_outputs(&self) -> Vec<Vec<u8>> {
        self.outputs.lock().unwrap().clone()
    }

    /// Get captured outputs as parsed responses
    pub fn get_responses(&self) -> Vec<JsonRpcResponse> {
        self.get_outputs()
            .iter()
            .filter_map(|data| serde_json::from_slice(data).ok())
            .collect()
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn read_message(&mut self) -> Result<Vec<u8>, TransportError> {
        match self.inputs.lock().unwrap().pop_front() {
            Some(data) => {
                eprintln!("[MockTransport] read_message: read {} bytes", data.len());
                Ok(data)
            }
            None => {
                eprintln!("[MockTransport] read_message: no more inputs, returning Closed");
                Err(TransportError::Closed)
            }
        }
    }

    async fn write_message(&mut self, data: &[u8]) -> Result<(), TransportError> {
        eprintln!(
            "[MockTransport] write_message: writing {} bytes to outputs",
            data.len()
        );
        self.outputs.lock().unwrap().push(data.to_vec());
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), TransportError> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        // This mock is always "connected" until inputs are exhausted.
        true
    }
}
