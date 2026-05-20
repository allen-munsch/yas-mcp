//! W3C Trace Context propagation.
//!
//! Implements the [W3C Trace Context](https://www.w3.org/TR/trace-context/) standard
//! for propagating trace context across HTTP calls (MCP → upstream API).
//!
//! Structured logging with correlation IDs ensures every log line is traceable.

use std::fmt;
use uuid::Uuid;

/// W3C Trace Context — propagated via `traceparent` header.
///
/// Format: `{version}-{trace_id}-{span_id}-{trace_flags}`
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub sampled: bool,
}

impl TraceContext {
    /// Create a new trace context with a random trace_id and span_id
    pub fn new_root() -> Self {
        Self {
            trace_id: generate_id(32),
            span_id: generate_id(16),
            sampled: true,
        }
    }

    /// Create a new span within an existing trace
    pub fn new_child(parent: &TraceContext) -> Self {
        Self {
            trace_id: parent.trace_id.clone(),
            span_id: generate_id(16),
            sampled: parent.sampled,
        }
    }

    /// Parse from a `traceparent` header value
    pub fn from_header(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        Some(Self {
            trace_id: parts[1].to_string(),
            span_id: parts[2].to_string(),
            sampled: parts[3] == "01",
        })
    }

    /// Render as a `traceparent` header value
    pub fn to_header(&self) -> String {
        format!(
            "00-{}-{}-{}",
            self.trace_id,
            self.span_id,
            if self.sampled { "01" } else { "00" }
        )
    }
}

impl fmt::Display for TraceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "trace_id={} span_id={}", self.trace_id, self.span_id)
    }
}

/// Generate a random lowercase hex ID of a given length (in hex chars)
fn generate_id(hex_length: usize) -> String {
    let num_uuids = (hex_length + 31) / 32; // Each UUID gives 32 hex chars
    let mut id = String::with_capacity(num_uuids * 32);
    for _ in 0..num_uuids {
        id.push_str(&Uuid::new_v4().to_string().replace('-', ""));
    }
    id.truncate(hex_length);
    id
}

/// Correlation ID — a unique identifier for a request lifecycle.
/// Different from trace_id: correlation_id is per-request, trace_id spans requests.
#[derive(Debug, Clone)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    /// Generate a new correlation ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_roundtrip() {
        let ctx = TraceContext::new_root();
        let header = ctx.to_header();
        let parsed = TraceContext::from_header(&header).unwrap();

        assert_eq!(ctx.trace_id, parsed.trace_id);
        assert_eq!(ctx.span_id, parsed.span_id);
        assert_eq!(ctx.sampled, parsed.sampled);
    }

    #[test]
    fn test_child_spans_share_trace_id() {
        let root = TraceContext::new_root();
        let child = TraceContext::new_child(&root);

        assert_eq!(root.trace_id, child.trace_id);
        assert_ne!(root.span_id, child.span_id);
        assert_eq!(root.sampled, child.sampled);
    }

    #[test]
    fn test_correlation_id_unique() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::new();
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_parse_valid_header() {
        let ctx = TraceContext::from_header(
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        )
        .unwrap();
        assert_eq!(ctx.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(ctx.span_id, "b7ad6b7169203331");
        assert!(ctx.sampled);
    }
}
