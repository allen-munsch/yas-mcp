// src/internal/requester/http_requester.rs

use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use reqwest::Client;
use anyhow::{Result, anyhow, Context};
use tracing::info;
use serde_json::Value;

use crate::internal::config::config::EndpointConfig;
use crate::internal::requester::RouteExecutor;

/// HTTP response structure
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
}

/// HTTPRequester handles both request building and execution
#[derive(Clone)]
pub struct HttpRequester {
    client: Client,
    service_cfg: EndpointConfig,
}

impl HttpRequester {
    /// Create a new HTTPRequester with default configuration
    pub fn new(service_cfg: &EndpointConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            service_cfg: service_cfg.clone(),
        })
    }

    /// Set timeout for the HTTP client
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.client = Client::builder()
            .timeout(timeout)
            .build()
            .context("Failed to recreate HTTP client with new timeout")?;
        Ok(())
    }

    /// Build a route executor for a specific route configuration
    pub fn build_route_executor(&self, config: &crate::internal::requester::RouteConfig) -> Result<RouteExecutor> {
        let base_url = self.service_cfg.base_url.clone();
        let method = config.method.clone();
        let path = config.path.clone();
        let mut headers = config.headers.clone();
        
        // Add service-level headers
        for (key, value) in &self.service_cfg.headers {
            headers.entry(key.clone()).or_insert(value.clone());
        }

        let client = self.client.clone();

        // Create the executor closure - now wrapped in Arc for cloning
        let executor: RouteExecutor = Arc::new(move |params_json: &str| {
            let base_url = base_url.clone();
            let method = method.clone();
            let path = path.clone();
            let headers = headers.clone();
            let client = client.clone();
            
            // Move the string into the async block to fix lifetime issues
            let params_json = params_json.to_string();

            Box::pin(async move {
                let params: serde_json::Value = serde_json::from_str(&params_json)
                    .context("Failed to parse parameters as JSON")?;

                // Build the full URL
                let mut url = format!("{}{}", base_url, path);

                // Handle path parameters
                if let Some(param_obj) = params.as_object() {
                    for (key, value) in param_obj {
                        if let serde_json::Value::String(str_value) = value {
                            let placeholder = format!("{{{}}}", key);
                            if url.contains(&placeholder) {
                                url = url.replace(&placeholder, str_value);
                            }
                        }
                    }
                }

                // Build request
                let mut request_builder = match method.as_str() {
                    "GET" => client.get(&url),
                    "POST" => client.post(&url),
                    "PUT" => client.put(&url),
                    "DELETE" => client.delete(&url),
                    "PATCH" => client.patch(&url),
                    _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
                };

                // Add headers
                for (key, value) in &headers {
                    request_builder = request_builder.header(key, value);
                }

                // Handle query parameters for GET requests
                if method == "GET" {
                    if let Some(param_obj) = params.as_object() {
                        for (key, value) in param_obj {
                            if let serde_json::Value::String(str_value) = value {
                                // Only add as query param if not used as path param
                                if !path.contains(&format!("{{{}}}", key)) {
                                    request_builder = request_builder.query(&[(key, str_value)]);
                                }
                            }
                        }
                    }
                } else {
                    // For non-GET requests, send params as JSON body
                    if !params.is_null() {
                        request_builder = request_builder.json(&params);
                    }
                }

                info!("Executing request: {} {}", method, url);

                // Execute request
                let response = request_builder.send().await
                    .context("Failed to execute HTTP request")?;

                Self::process_response(response).await
            })
        });

        Ok(executor)
    }

    /// Process the HTTP response into our standard format
    async fn process_response(response: reqwest::Response) -> Result<HttpResponse> {
        let status_code = response.status().as_u16();
        
        // Clone headers before consuming the response
        let headers_map: HashMap<String, String> = response.headers()
            .iter()
            .filter_map(|(key, value)| {
                value.to_str().ok().map(|v| (key.as_str().to_string(), v.to_string()))
            })
            .collect();
        
        // Read response body
        let body = response.bytes().await
            .context("Failed to read response body")?
            .to_vec();

        Ok(HttpResponse {
            status_code,
            body,
            headers: headers_map,
        })
    }

    /// Direct execution method for testing or manual use
    pub async fn execute_direct(
        &self,
        method: &str,
        url: &str,
        headers: Option<HashMap<String, String>>,
        body: Option<Value>,
    ) -> Result<HttpResponse> {
        let mut request_builder = match method {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "DELETE" => self.client.delete(url),
            "PATCH" => self.client.patch(url),
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };

        // Add headers
        if let Some(headers_map) = headers {
            for (key, value) in headers_map {
                request_builder = request_builder.header(&key, &value);
            }
        }

        // Add body for non-GET requests
        if let Some(body_data) = body {
            if method != "GET" {
                request_builder = request_builder.json(&body_data);
            }
        }

        let response = request_builder.send().await
            .context("Failed to execute HTTP request")?;

        Self::process_response(response).await
    }
}