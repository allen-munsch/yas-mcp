use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Stdin, Stdout};

use async_trait::async_trait;

use super::{Transport, TransportError};

/// STDIO transport for MCP over stdin/stdout
pub struct StdioTransport {
    stdin: BufReader<Stdin>,
    stdout: Stdout,
    buffer: String,
}

impl StdioTransport {
    pub fn new() -> Self {
        Self {
            stdin: BufReader::new(tokio::io::stdin()),
            stdout: tokio::io::stdout(),
            buffer: String::new(),
        }
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn read_message(&mut self) -> Result<Vec<u8>, TransportError> {
        self.buffer.clear();
        let bytes_read = self.stdin.read_line(&mut self.buffer).await?;

        if bytes_read == 0 {
            return Err(TransportError::Closed);
        }

        // Trim trailing newline
        let trimmed = self.buffer.trim_end();
        Ok(trimmed.as_bytes().to_vec())
    }

    async fn write_message(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.stdout.write_all(data).await?;
        self.stdout.write_all(b"\n").await?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), TransportError> {
        self.stdout.flush().await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true // STDIO is always "connected" while process runs
    }
}
