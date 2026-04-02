use anyhow::{anyhow, Context, Result};
use reqwest::{Client, header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT}};
use uuid::Uuid;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

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
        info!("Initializing HttpRequester with base_url: '{}'", service_cfg.base_url);
        info!("Headers in config: {:?}", service_cfg.headers);
        let mut default_headers = HeaderMap::new();
        for (k, v) in &service_cfg.headers {
            let lower_key = k.to_lowercase();
            // TODO: remove this
            debug!("{}: {}", lower_key, v);
            if let Ok(name) = HeaderName::try_from(lower_key.as_str()) {
                if let Ok(value) = HeaderValue::from_str(v) {
                    default_headers.insert(name, value);
                }
            }
        }
        if !default_headers.contains_key(USER_AGENT) {
            default_headers.insert(USER_AGENT, HeaderValue::from_static("yas-mcp-agent/0.0.1"));
        }

        let client = Client::builder()
            .default_headers(default_headers)
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
        debug!("service_cfg: {:?}", self.service_cfg);

        let base_url = self.service_cfg.base_url.clone();
        let method = config.method.clone();
        let path = config.path.clone();
        let mut static_headers = config.headers.clone();

        // Capture known param names from config to separate them
        // Fields are Vec<String>, so we just clone them
        let known_header_params = config.method_config.header_params.clone();
        let known_query_params = config.method_config.query_params.clone();
        debug!("known_header_params: {:?}", known_header_params);
        debug!("known_query_params: {:?}", known_query_params);
        debug!("static headers:");

        for (key, value) in &self.service_cfg.headers {
            debug!("\t{}: {}", key, value);
            let lower_key = key.to_lowercase();
            static_headers.entry(lower_key).or_insert(value.clone());
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
                let request_id = Uuid::new_v4().to_string();
                info!(request_id = %request_id, "Starting request execution");

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
                debug!(request_id = %request_id, url = %url, "URL after path param subsitution");

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
                debug!(request_id = %request_id, headers = ?static_headers, "Static headers applied");

                // 4. Handle Dynamic Headers
                for header_key in &known_header_params {
                    if let Some(val) = active_params.remove(header_key) {
                        let header_val  = val.as_str().map(|s| s.to_string()).unwrap_or_else(|| val.to_string());
                        request_builder = request_builder.header(header_key.as_str(), header_val.clone());
                        debug!(request_id = %request_id, header_key = %header_key, header_val = %header_val, "Dynamic header applied");
                    }
                }

                // 5. Handle Query Params (Explicit list)
                for query_key in &known_query_params {
                    if let Some(val) = active_params.remove(query_key) {
                        let query_val = val.as_str().map(|s| s.to_string()).unwrap_or_else(|| val.to_string());
                        request_builder = request_builder.query(&[(query_key, query_val.clone())]);
                        debug!(request_id = %request_id, query_key = %query_key, query_val = %val, "Query param applied");
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

                let response = request_builder
                    .send()
                    .await
                    .map_err(|e| {
                        error!(request_id = %request_id, error = %e, "HTTP request failed");
                        e
                    })
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

        let body_bytes = response
            .bytes()
            .await
            .context("Failed to read response body")?;
        let body_preview = String::from_utf8_lossy(&body_bytes);
        let truncated = if body_preview.len() > 500 {
            format!("{}...<truncated>", &body_preview[..500])
        } else {
            body_preview.to_string()
        };

        if status_code >= 400 {
            error!(
                status_code = status_code,
                headers = ?headers_map,
                body = %truncated,
                "HTTP error response"
            );
        } else {
            debug!(
                status_code = status_code,
                headers = ?headers_map,
                body = %truncated,
                "HTTP success response"
            )
        }

        Ok(HttpResponse {
            status_code,
            body: body_bytes.to_vec(),
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
