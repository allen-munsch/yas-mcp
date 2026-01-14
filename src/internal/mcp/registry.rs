use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::internal::server::tool::handler::ToolExecutor;
use rmcp::model::Tool;

pub struct RegisteredTool {
    pub metadata: Tool,
    pub executor: ToolExecutor,
}

/// Thread-safe tool registry that can be shared across transports
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<RegisteredTool>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }
    pub fn register(&self, name: String, tool: RegisteredTool) {
        self.tools.write().unwrap().insert(name, Arc::new(tool));
    }
    pub fn get(&self, name: &str) -> Option<Arc<RegisteredTool>> {
        self.tools.read().unwrap().get(name).cloned()
    }
    pub fn list_metadata(&self) -> Vec<Tool> {
        self.tools
            .read()
            .unwrap()
            .values()
            .map(|tool| tool.metadata.clone())
            .collect()
    }
    pub fn count(&self) -> usize {
        self.tools.read().unwrap().len()
    }
}
