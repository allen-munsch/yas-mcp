use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use crate::internal::config::_config::EndpointConfig;
use crate::internal::requester::RouteExecutor;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
}

#[derive(Clone)]
pub struct HttpRequester {
    client: Client,
    service_cfg: EndpointConfig,
}

impl HttpRequester {
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

    pub fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.client = Client::builder()
            .timeout(timeout)
            .build()
            .context("Failed to recreate HTTP client with new timeout")?;
        Ok(())
    }

    pub fn build_route_executor(
        &self,
        config: &crate::internal::requester::RouteConfig,
    ) -> Result<RouteExecutor> {
        let base_url = self.service_cfg.base_url.clone();
        let method = config.method.clone();
        let path = config.path.clone();
        let mut static_headers = config.headers.clone();

        // Capture known param names from config to separate them
        // Fields are Vec<String>, so we just clone them
        let known_header_params = config.method_config.header_params.clone();
        let known_query_params = config.method_config.query_params.clone();

        for (key, value) in &self.service_cfg.headers {
            static_headers.entry(key.clone()).or_insert(value.clone());
        }

        let client = self.client.clone();

        let executor: RouteExecutor = Arc::new(move |params_json: &str| {
            let base_url = base_url.clone();
            let method = method.clone();
            let path = path.clone();
            let static_headers = static_headers.clone();
            let client = client.clone();

            // Capture these for the closure
            let known_header_params = known_header_params.clone();
            let known_query_params = known_query_params.clone();

            let params_json = params_json.to_string();

            Box::pin(async move {
                // Parse the main input
                let params_value: serde_json::Value = serde_json::from_str(&params_json)
                    .context("Failed to parse parameters as JSON")?;

                // Convert to object for manipulation (so we can remove fields as we use them)
                let mut active_params = params_value.as_object().cloned().unwrap_or_default();

                // 1. Build URL & Handle Path Params
                // (Iterate all params to see if they match URL placeholders)
                let mut url = format!("{}{}", base_url, path);

                // We collect keys to remove to avoid modification during iteration
                let mut used_keys = Vec::new();
                for (key, value) in &active_params {
                    if let serde_json::Value::String(str_value) = value {
                        let placeholder = format!("{{{}}}", key);
                        if url.contains(&placeholder) {
                            url = url.replace(&placeholder, str_value);
                            used_keys.push(key.clone());
                        }
                    }
                }
                // Remove path params from map so they aren't sent in body/query
                for k in used_keys {
                    active_params.remove(&k);
                }

                // 2. Build Request
                let mut request_builder = match method.as_str() {
                    "GET" => client.get(&url),
                    "POST" => client.post(&url),
                    "PUT" => client.put(&url),
                    "DELETE" => client.delete(&url),
                    "PATCH" => client.patch(&url),
                    _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
                };

                // 3. Add Static Headers
                for (key, value) in &static_headers {
                    request_builder = request_builder.header(key, value);
                }

                // 4. Handle Dynamic Headers
                for header_key in &known_header_params {
                    if let Some(val) = active_params.remove(header_key) {
                        if let Some(s) = val.as_str() {
                            // Fix: Use as_str() because header() expects &str, not &String
                            request_builder = request_builder.header(header_key.as_str(), s);
                        } else {
                            // Convert numbers/bools to string for header
                            request_builder =
                                request_builder.header(header_key.as_str(), val.to_string());
                        }
                    }
                }

                // 5. Handle Query Params (Explicit list)
                for query_key in &known_query_params {
                    if let Some(val) = active_params.remove(query_key) {
                        if let Some(s) = val.as_str() {
                            request_builder = request_builder.query(&[(query_key, s)]);
                        } else {
                            request_builder =
                                request_builder.query(&[(query_key, val.to_string())]);
                        }
                    }
                }

                // 6. Handle Remaining Params (Body vs Query Fallback)
                if !active_params.is_empty() {
                    if method == "GET" {
                        // For GET, anything leftover goes to query (fallback behavior)
                        request_builder = request_builder.query(&active_params);
                    } else {
                        // For POST/PUT/PATCH, leftovers go to JSON body
                        request_builder = request_builder.json(&active_params);
                    }
                }

                info!("Executing request: {} {}", method, url);

                let response = request_builder
                    .send()
                    .await
                    .context("Failed to execute HTTP request")?;

                Self::process_response(response).await
            })
        });

        Ok(executor)
    }

    async fn process_response(response: reqwest::Response) -> Result<HttpResponse> {
        let status_code = response.status().as_u16();
        let headers_map: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(key, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (key.as_str().to_string(), v.to_string()))
            })
            .collect();

        let body = response
            .bytes()
            .await
            .context("Failed to read response body")?
            .to_vec();

        Ok(HttpResponse {
            status_code,
            body,
            headers: headers_map,
        })
    }

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

        if let Some(headers_map) = headers {
            for (key, value) in headers_map {
                request_builder = request_builder.header(&key, &value);
            }
        }

        if let Some(body_data) = body {
            if method != "GET" {
                request_builder = request_builder.json(&body_data);
            }
        }

        let response = request_builder
            .send()
            .await
            .context("Failed to execute HTTP request")?;

        Self::process_response(response).await
    }
}
