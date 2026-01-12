use async_trait::async_trait;
pub mod stdio;
#[cfg(any(test, feature = "test-utils"))]
pub mod mock;
pub mod runner;

/// Transport abstraction for different MCP communication channels
#[async_trait]
pub trait Transport: Send + Sync {
    /// Read the next message from the transport
    async fn read_message(&mut self) -> Result<Vec<u8>, TransportError>;
    
    /// Write a message to the transport
    async fn write_message(&mut self, data: &[u8]) -> Result<(), TransportError>;
    
    /// Flush any buffered data
    async fn flush(&mut self) -> Result<(), TransportError>;
    
    /// Check if transport is still connected
    fn is_connected(&self) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Connection closed")]
    Closed,
    #[error("Invalid frame: {0}")]
    InvalidFrame(String),
}