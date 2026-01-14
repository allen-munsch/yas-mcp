//! Test fixtures for MCP protocol testing

pub mod openapi;
pub mod requests;
pub mod responses;

/// Common test configuration
pub struct TestConfig {
    pub openapi_path: String,
    pub adjustments_path: Option<String>,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            openapi_path: "examples/todo-app/openapi.yaml".to_string(),
            adjustments_path: None,
        }
    }
}
