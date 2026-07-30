//! A2A Protocol Types
//!
//! Core types matching the A2A Protocol v1.0 specification.
//! Reference: https://a2a-protocol.org/latest/specification/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Agent Card ────────────────────────────────────────────────────────────

/// Top-level Agent Card — the discovery document at `/.well-known/agent-card.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    pub version: String,
    #[serde(default)]
    pub capabilities: AgentCapabilities,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<AgentSkill>,
    #[serde(default = "default_input_modes")]
    pub default_input_modes: Vec<String>,
    #[serde(default = "default_output_modes")]
    pub default_output_modes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<AgentIcons>,
}

fn default_input_modes() -> Vec<String> {
    vec![
        "text".into(),
        "text/plain".into(),
        "application/json".into(),
    ]
}

fn default_output_modes() -> Vec<String> {
    vec![
        "text".into(),
        "text/plain".into(),
        "application/json".into(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProvider {
    pub organization: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCapabilities {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub push_notifications: bool,
    #[serde(default)]
    pub state_transition_history: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    #[serde(default = "default_skill_input_modes")]
    pub input_modes: Vec<String>,
    #[serde(default = "default_skill_output_modes")]
    pub output_modes: Vec<String>,
}

fn default_skill_input_modes() -> Vec<String> {
    vec!["application/json".into()]
}

fn default_skill_output_modes() -> Vec<String> {
    vec!["application/json".into()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIcons {
    pub src: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_type: Option<String>,
}

// ── Task ───────────────────────────────────────────────────────────────────

/// Represents a single A2A task with full lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<TaskEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<TaskMessage>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Task lifecycle states per A2A spec
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    #[serde(rename = "submitted")]
    Submitted,
    #[serde(rename = "working")]
    Working,
    #[serde(rename = "input-required")]
    InputRequired,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "canceled")]
    Canceled,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "rejected")]
    Rejected,
}

impl TaskState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Canceled | TaskState::Failed | TaskState::Rejected
        )
    }
}

/// A message within a task (user input, agent response, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub role: String,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// A single event in the task history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub state: TaskState,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ── Message Parts ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Part {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "data")]
    Data { data: serde_json::Value },
    #[serde(rename = "file")]
    File { file: FileReference },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

// ── Artifacts ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// ── Request/Response Types ─────────────────────────────────────────────────

/// Request body for `tasks/send` and `tasks/sendSubscribe`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSendRequest {
    pub id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub message: TaskMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "contextId")]
    pub context_id: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Response for `tasks/send` (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSendResponse {
    pub id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "contextId")]
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
}

/// Query params for `tasks/get`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGetParams {
    pub id: String,
    #[serde(rename = "sessionId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Request for `tasks/cancel`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCancelRequest {
    pub id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
}

/// Push notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationConfig {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_is_terminal() {
        assert!(!TaskState::Submitted.is_terminal());
        assert!(!TaskState::Working.is_terminal());
        assert!(!TaskState::InputRequired.is_terminal());
        assert!(TaskState::Completed.is_terminal());
        assert!(TaskState::Canceled.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(TaskState::Rejected.is_terminal());
    }

    #[test]
    fn test_agent_card_serialization() {
        let card = AgentCard {
            name: "Test Agent".into(),
            description: "A test agent".into(),
            url: "http://localhost:3000".into(),
            provider: Some(AgentProvider {
                organization: "Test Org".into(),
                url: Some("https://example.com".into()),
            }),
            version: "1.0.0".into(),
            capabilities: AgentCapabilities::default(),
            skills: vec![AgentSkill {
                id: "test_skill".into(),
                name: "Test Skill".into(),
                description: Some("A test skill".into()),
                tags: vec!["test".into()],
                examples: vec!["Run test".into()],
                input_modes: vec!["application/json".into()],
                output_modes: vec!["application/json".into()],
            }],
            default_input_modes: default_input_modes(),
            default_output_modes: default_output_modes(),
            documentation: None,
            icons: None,
        };

        let json = serde_json::to_string_pretty(&card).unwrap();
        assert!(json.contains("Test Agent"));
        assert!(json.contains("test_skill"));

        let parsed: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test Agent");
        assert_eq!(parsed.skills.len(), 1);
    }

    #[test]
    fn test_part_serialization() {
        let text_part = Part::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&text_part).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"text\":\"hello\""));

        let data_part = Part::Data {
            data: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&data_part).unwrap();
        assert!(json.contains("\"type\":\"data\""));
    }

    #[test]
    fn test_task_state_serialization() {
        let states = vec![
            ("submitted", TaskState::Submitted),
            ("working", TaskState::Working),
            ("completed", TaskState::Completed),
            ("failed", TaskState::Failed),
            ("canceled", TaskState::Canceled),
        ];

        for (expected, state) in states {
            let json = serde_json::to_string(&state).unwrap();
            assert!(json.contains(expected), "Expected {expected} in {json}");
            let parsed: TaskState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, state);
        }
    }
}
