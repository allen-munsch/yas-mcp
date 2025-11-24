// src/internal/requester/types.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use super::http_requester::HttpResponse;

// Change RouteExecutor to be async
pub type RouteExecutor = Arc<dyn Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<HttpResponse, anyhow::Error>> + Send>> + Send + Sync>;

/// RouteConfig holds the configuration for a specific route
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    pub path: String,
    pub method: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub headers: HashMap<String, String>,
    pub parameters: HashMap<String, String>,
    /// Method specific configurations
    pub method_config: MethodConfig,
}

/// MethodConfig holds method-specific configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodConfig {
    /// For GET requests
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub query_params: Vec<String>,

    /// For multipart/form-data
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub form_fields: Vec<String>,

    /// For file uploads
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_upload: Option<FileUploadConfig>,
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

impl Default for MethodConfig {
    fn default() -> Self {
        Self {
            query_params: Vec::new(),
            form_fields: Vec::new(),
            file_upload: None,
        }
    }
}