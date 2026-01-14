use serde::{Deserialize, Serialize};

use crate::internal::mcp::protocol::{JsonRpcRequest, JsonRpcResponse};

/// Parses and generates Gemini-style MCP transcripts
pub struct TranscriptParser;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub direction: Direction,
    pub timestamp: Option<String>,
    #[serde(flatten)]
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Direction {
    #[serde(rename = ">>")]
    ClientToServer,
    #[serde(rename = "<<")]
    ServerToClient,
}

impl TranscriptParser {
    /// Parse a .jsonl transcript file
    pub fn parse_file(path: &str) -> Result<Vec<TranscriptEntry>, TranscriptError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse_string(&content)
    }

    /// Parse transcript from string
    pub fn parse_string(content: &str) -> Result<Vec<TranscriptEntry>, TranscriptError> {
        content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
            .map(|(i, line)| {
                serde_json::from_str(line).map_err(|e| TranscriptError::ParseError {
                    line: i + 1,
                    message: e.to_string(),
                })
            })
            .collect()
    }

    /// Generate transcript entries from a message exchange
    pub fn record_exchange(
        request: &JsonRpcRequest,
        response: &JsonRpcResponse,
    ) -> Vec<TranscriptEntry> {
        vec![
            TranscriptEntry {
                direction: Direction::ClientToServer,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                message: Message::Request(request.clone()),
            },
            TranscriptEntry {
                direction: Direction::ServerToClient,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                message: Message::Response(response.clone()),
            },
        ]
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error at line {line}: {message}")]
    ParseError { line: usize, message: String },
}
