// src/internal/requester/types.rs

use super::http_requester::HttpResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// Change RouteExecutor to be async
pub type RouteExecutor = Arc<
    dyn Fn(
            &str,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<HttpResponse, anyhow::Error>> + Send>,
        > + Send
        + Sync,
>;

/// RouteConfig holds the configuration for a specific route
#[derive(Debug, Clone, Default)]
pub struct RouteConfig {
    pub path: String,
    pub method: String,
    pub description: String,
    pub headers: HashMap<String, String>,
    pub parameters: HashMap<String, String>,
    pub method_config: MethodConfig,
}

/// MethodConfig holds method-specific configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MethodConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_params: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_params: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form_fields: Vec<String>,
    pub file_upload: Option<String>,
}

/// FileUploadConfig holds configuration for file uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadConfig {
    pub field_name: String,
    pub allowed_types: Vec<String>,
    pub max_size: i64,
}

/// RequestResult holds the result of a request
#[derive(Debug)]
pub struct RequestResult {
    pub status_code: u16,
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
    pub error: Option<String>,
}

impl RouteConfig {
    /// Create a new RouteConfig with minimal required fields
    pub fn new(path: String, method: String, description: String) -> Self {
        Self {
            path,
            method,
            description,
            headers: HashMap::new(),
            parameters: HashMap::new(),
            method_config: MethodConfig::default(),
        }
    }
}
