use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteFieldUpdate {
    pub method: String,
    #[serde(rename = "new_description")]
    pub new_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDescription {
    pub path: String,
    pub updates: Vec<RouteFieldUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteSelection {
    pub path: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAdjustments {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<RouteDescription>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<RouteSelection>,
}
