//! Prometheus Metrics
//!
//! Defines and registers all application metrics.
//! Exposed at `GET /metrics` on the HTTP server.
//!
//! # Metrics
//!
//! | Name | Type | Labels | Description |
//! |------|------|--------|-------------|
//! | `yas_mcp_tool_calls_total` | Counter | `tool`, `method` | Total tool calls |
//! | `yas_mcp_tool_errors_total` | Counter | `tool`, `method`, `status` | Failed tool calls |
//! | `yas_mcp_tool_duration_seconds` | Histogram | `tool`, `method` | Tool call latency |
//! | `yas_mcp_upstream_requests_total` | Counter | `host`, `method`, `status` | Upstream API calls |
//! | `yas_mcp_upstream_duration_seconds` | Histogram | `host`, `method` | Upstream API latency |
//! | `yas_mcp_active_tools` | Gauge | — | Currently registered tools |
//! | `yas_mcp_a2a_tasks_total` | Counter | `state` | A2A task state transitions |
//! | `yas_mcp_build_info` | Gauge | `version` | Build info (constant 1) |

use prometheus::{
    self, register_counter_vec_with_registry, register_gauge_vec_with_registry,
    register_histogram_vec_with_registry, CounterVec, GaugeVec, HistogramVec, Registry,
    TextEncoder,
};
use std::sync::OnceLock;
use tracing::info;

/// Global metric registry (lazily initialized)
static METRICS: OnceLock<Metrics> = OnceLock::new();

/// All application metrics in one struct
pub struct Metrics {
    pub registry: Registry,

    /// Total MCP tool calls (labeled by tool name, HTTP method)
    pub tool_calls: CounterVec,
    /// Failed MCP tool calls (labeled by tool, method, status code)
    pub tool_errors: CounterVec,
    /// MCP tool call latency histogram
    pub tool_duration: HistogramVec,

    /// Upstream API HTTP requests
    pub upstream_requests: CounterVec,
    /// Upstream API request latency
    pub upstream_duration: HistogramVec,

    /// Currently registered tools count
    pub active_tools: GaugeVec,

    /// A2A task state transitions
    pub a2a_tasks: CounterVec,

    /// Build version info
    pub build_info: GaugeVec,
}

impl Metrics {
    /// Initialize and register all metrics.
    /// Must be called once before `get()`.
    pub fn init(version: &str) -> &'static Self {
        METRICS.get_or_init(|| {
            let registry = Registry::new();

            let tool_calls = register_counter_vec_with_registry!(
                "yas_mcp_tool_calls_total",
                "Total MCP tool calls",
                &["tool", "method"],
                registry
            )
            .unwrap();

            let tool_errors = register_counter_vec_with_registry!(
                "yas_mcp_tool_errors_total",
                "Failed MCP tool calls",
                &["tool", "method", "status"],
                registry
            )
            .unwrap();

            let tool_duration = register_histogram_vec_with_registry!(
                "yas_mcp_tool_duration_seconds",
                "MCP tool call latency in seconds",
                &["tool", "method"],
                prometheus::exponential_buckets(0.001, 2.0, 15).unwrap(),
                registry
            )
            .unwrap();

            let upstream_requests = register_counter_vec_with_registry!(
                "yas_mcp_upstream_requests_total",
                "Upstream API HTTP requests",
                &["host", "method", "status"],
                registry
            )
            .unwrap();

            let upstream_duration = register_histogram_vec_with_registry!(
                "yas_mcp_upstream_duration_seconds",
                "Upstream API request latency",
                &["host", "method"],
                prometheus::exponential_buckets(0.001, 2.0, 15).unwrap(),
                registry
            )
            .unwrap();

            let active_tools = register_gauge_vec_with_registry!(
                "yas_mcp_active_tools",
                "Number of currently registered MCP tools",
                &[],
                registry
            )
            .unwrap();

            let a2a_tasks = register_counter_vec_with_registry!(
                "yas_mcp_a2a_tasks_total",
                "A2A task state transitions",
                &["state"],
                registry
            )
            .unwrap();

            let build_info = register_gauge_vec_with_registry!(
                "yas_mcp_build_info",
                "Build information",
                &["version"],
                registry
            )
            .unwrap();

            // Set build info
            build_info.with_label_values(&[version]).set(1.0);

            info!("Prometheus metrics initialized (v{})", version);

            Metrics {
                registry,
                tool_calls,
                tool_errors,
                tool_duration,
                upstream_requests,
                upstream_duration,
                active_tools,
                a2a_tasks,
                build_info,
            }
        })
    }

    /// Get the global metrics instance.
    /// Panics if `init()` hasn't been called.
    pub fn get() -> &'static Self {
        METRICS
            .get()
            .expect("Metrics not initialized — call Metrics::init() first")
    }

    /// Encode all metrics as Prometheus text format
    pub fn encode(&self) -> Result<String, String> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder
            .encode_to_string(&metric_families)
            .map_err(|e| format!("Failed to encode metrics: {e}"))
    }

    /// Record a tool call with its duration
    pub fn record_tool_call(&self, tool: &str, method: &str, duration_secs: f64) {
        self.tool_calls
            .with_label_values(&[tool, method])
            .inc();
        self.tool_duration
            .with_label_values(&[tool, method])
            .observe(duration_secs);
    }

    /// Record a tool execution error
    pub fn record_tool_error(&self, tool: &str, method: &str, status_code: u16) {
        self.tool_errors
            .with_label_values(&[tool, method, &status_code.to_string()])
            .inc();
    }

    /// Record an upstream API call (from yas-mcp to the proxied API)
    pub fn record_upstream_call(
        &self,
        host: &str,
        method: &str,
        status_code: u16,
        duration_secs: f64,
    ) {
        self.upstream_requests
            .with_label_values(&[host, method, &status_code.to_string()])
            .inc();
        self.upstream_duration
            .with_label_values(&[host, method])
            .observe(duration_secs);
    }

    /// Set the active tools gauge
    pub fn set_active_tools(&self, count: f64) {
        self.active_tools.with_label_values(&[]).set(count);
    }

    /// Record an A2A task state transition
    pub fn record_a2a_task(&self, state: &str) {
        self.a2a_tasks.with_label_values(&[state]).inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_init() {
        let m = Metrics::init("0.1.0-test");
        assert!(m.encode().is_ok());
    }

    #[test]
    fn test_record_tool_call() {
        let m = Metrics::init("0.1.0-test");
        m.record_tool_call("get_users", "GET", 0.042);
        m.record_tool_call("get_users", "GET", 0.031);
        m.record_tool_call("post_projects", "POST", 0.120);

        let encoded = m.encode().unwrap();
        assert!(encoded.contains("yas_mcp_tool_calls_total"));
        assert!(encoded.contains("yas_mcp_tool_duration_seconds"));
    }

    #[test]
    fn test_record_tool_error() {
        let m = Metrics::init("0.1.0-test");
        m.record_tool_error("bad_tool", "POST", 500);
        m.record_tool_error("bad_tool", "POST", 503);

        let encoded = m.encode().unwrap();
        assert!(encoded.contains("yas_mcp_tool_errors_total"));
    }

    #[test]
    fn test_record_upstream_call() {
        let m = Metrics::init("0.1.0-test");
        m.record_upstream_call("api.example.com", "GET", 200, 0.050);
        m.record_upstream_call("api.example.com", "POST", 201, 0.150);
        m.record_upstream_call("api.example.com", "GET", 500, 0.300);

        let encoded = m.encode().unwrap();
        assert!(encoded.contains("yas_mcp_upstream_requests_total"));
        assert!(encoded.contains("yas_mcp_upstream_duration_seconds"));
    }

    #[test]
    fn test_active_tools() {
        let m = Metrics::init("0.1.0-test");
        m.set_active_tools(42.0);
        let encoded = m.encode().unwrap();
        assert!(encoded.contains("yas_mcp_active_tools"));
    }

    #[test]
    fn test_a2a_tasks() {
        let m = Metrics::init("0.1.0-test");
        m.record_a2a_task("submitted");
        m.record_a2a_task("working");
        m.record_a2a_task("completed");
        m.record_a2a_task("failed");

        let encoded = m.encode().unwrap();
        assert!(encoded.contains("yas_mcp_a2a_tasks_total"));
    }

    #[test]
    fn test_build_info() {
        // Init with a unique version — but OnceLock means first init wins
        let m = Metrics::init("test-build-info");
        let encoded = m.encode().unwrap();
        assert!(encoded.contains("yas_mcp_build_info"));
        // The version label should be present (whatever it is)
        assert!(encoded.contains("version="));
    }

    #[test]
    fn test_encode_is_valid_prometheus() {
        let m = Metrics::init("0.1.0-test");
        m.record_tool_call("test_tool", "GET", 0.1);
        let encoded = m.encode().unwrap();

        // Basic Prometheus format checks
        assert!(encoded.contains("# HELP yas_mcp_"));
        assert!(encoded.contains("# TYPE yas_mcp_"));
    }
}
