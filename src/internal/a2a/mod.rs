//! A2A Protocol Integration
//!
//! Google's Agent-to-Agent (A2A) protocol — peer-to-peer agent communication.
//! yas-mcp implements both MCP (client-server) and A2A (peer-to-peer)
//! on the same tool registry — zero duplication of API surface.

pub mod agent_card;
pub mod router;
pub mod sse;
pub mod task_store;
pub mod types;

// Re-export key types
pub use agent_card::AgentCardGenerator;
pub use router::{
    A2aState, agent_card_handler, tasks_cancel_handler, tasks_get_handler, tasks_send_handler,
    tasks_send_subscribe_handler,
};
pub use task_store::TaskStore;
pub use types::AgentCard;
