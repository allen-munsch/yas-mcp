use anyhow::{Context, Result};
use serde_yaml;
use std::fs;
use tracing::{debug, info, warn};

use crate::internal::models::adjustments::McpAdjustments;

/// Adjuster provides filtering and description overrides based on YAML configuration
pub struct Adjuster {
    adjustments: McpAdjustments,
}

impl Adjuster {
    /// Create a new Adjuster instance
    pub fn new() -> Self {
        Self {
            adjustments: McpAdjustments {
                descriptions: Vec::new(),
                routes: Vec::new(),
            },
        }
    }

    /// Load adjustments from a YAML file
    pub fn load(&mut self, file_path: &str) -> Result<()> {
        if file_path.is_empty() {
            info!("No adjustments file provided");
            return Ok(());
        }

        info!("Loading adjustments from file: {}", file_path);

        // Check if file exists first
        if !fs::metadata(file_path).is_ok() {
            warn!("Adjustments file not found: {}", file_path);
            return Ok(()); // Return Ok if file doesn't exist (matching Go behavior)
        }

        let data = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read adjustments file: {}", file_path))?;

        let adjustments: McpAdjustments = serde_yaml::from_str(&data).with_context(|| {
            format!("Failed to parse YAML from adjustments file: {}", file_path)
        })?;

        debug!("Loaded adjustments: {:?}", adjustments);
        self.adjustments = adjustments;
        Ok(())
    }

    /// Check if a route with the given method exists in MCP
    /// Returns true if the route/method IS in the selected routes
    pub fn exists_in_mcp(&self, route: &str, method: &str) -> bool {
        debug!(
            "Checking if route '{}' method '{}' exists in MCP",
            route, method
        );

        // If no routes are specified in adjustments, allow ALL routes
        if self.adjustments.routes.is_empty() {
            debug!("No route filtering configured - allowing all routes");
            return true;
        }

        debug!("Available route selections: {:?}", self.adjustments.routes);

        // Look through all route selections
        for selection in &self.adjustments.routes {
            // Check if this path matches (handle trailing slashes)
            let normalized_selection_path = selection.path.trim_end_matches('/');
            let normalized_route = route.trim_end_matches('/');

            debug!(
                "Comparing: selection='{}' vs route='{}'",
                normalized_selection_path, normalized_route
            );

            if normalized_selection_path == normalized_route {
                // Check if the method is in the list of selected methods
                let method_exists = selection
                    .methods
                    .iter()
                    .any(|m| m.to_uppercase() == method.to_uppercase());
                debug!(
                    "Path match found! Method '{}' exists: {}",
                    method, method_exists
                );
                return method_exists;
            }
        }

        debug!("Route '{}' not found in adjustments", route);
        false // Route not found in adjustments
    }

    /// Get the updated description for a route/method if it exists
    pub fn get_description(&self, route: &str, method: &str, original_desc: &str) -> String {
        if self.adjustments.descriptions.is_empty() {
            return original_desc.to_string(); // Return original if no adjustments
        }

        debug!("Looking for description override for {} {}", method, route);

        // Look through all route descriptions
        for desc in &self.adjustments.descriptions {
            if desc.path == route {
                // Look through all updates for this route
                for update in &desc.updates {
                    if update.method == method {
                        debug!("Found description override for {} {}", method, route);
                        return update.new_description.clone();
                    }
                }
                break; // Found the route but no matching method
            }
        }

        original_desc.to_string()
    }

    /// Get the number of route selections in the adjuster
    pub fn get_routes_count(&self) -> usize {
        self.adjustments.routes.len()
    }
}

impl Default for Adjuster {
    fn default() -> Self {
        Self::new()
    }
}
