//! A2A Router — Axum route handlers for A2A protocol endpoints
//!
//! Co-located with the MCP HTTP server. Handles:
//! - `/.well-known/agent-card.json` — Agent discovery
//! - `/a2a/tasks/send` — Task submission
//! - `/a2a/tasks/get` — Task status retrieval
//! - `/a2a/tasks/cancel` — Task cancellation

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::info;

use crate::internal::a2a::agent_card::AgentCardGenerator;
use crate::internal::a2a::task_store::TaskStore;
use crate::internal::a2a::types::*;
use crate::internal::config::AppConfig;
use crate::internal::mcp::registry::ToolRegistry;
use crate::internal::requester::types::RouteConfig;
use std::sync::Arc;

/// Shared state for A2A route handlers
#[derive(Clone)]
pub struct A2aState {
    pub config: AppConfig,
    pub tool_registry: Arc<ToolRegistry>,
    pub route_configs: Arc<Vec<RouteConfig>>,
    pub task_store: Arc<TaskStore>,
}

/// GET /.well-known/agent-card.json
pub async fn agent_card_handler(State(state): State<A2aState>) -> impl IntoResponse {
    let tools = state.tool_registry.list_metadata();
    let card =
        AgentCardGenerator::generate(&state.config, &state.tool_registry, &state.route_configs);
    let serialized = serde_json::to_value(&card).unwrap_or_default();

    info!(
        "Serving Agent Card: {} with {} skills",
        card.name,
        tools.len()
    );

    (StatusCode::OK, Json(serialized))
}

/// POST /a2a/tasks/send
pub async fn tasks_send_handler(
    State(state): State<A2aState>,
    Json(request): Json<TaskSendRequest>,
) -> impl IntoResponse {
    let span = tracing::info_span!("a2a.task_send", task.id = %request.id);
    let _enter = span.enter();
    info!("A2A tasks/send: task_id={}", request.id);

    // Create a task in the store
    let task = state
        .task_store
        .create(&request.session_id, &request.message);

    // Transition to working
    if let Err(e) = state.task_store.start_working(&task.id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e
            })),
        );
    }

    // Extract skill name and parameters from the message
    let skill_name = extract_skill_name(&request.message);
    let params = extract_parameters(&request.message);

    // Spawn tool execution in background — return working state immediately
    let task_id = task.id.clone();
    let state_bg = state.clone();
    let skill_bg = skill_name.clone();
    let params_bg = params.clone();
    tokio::spawn(async move {
        match skill_bg {
            Some(skill) => match execute_tool(&state_bg, &task_id, &skill, &params_bg).await {
                Ok(artifacts) => {
                    let _ = state_bg.task_store.complete(&task_id, artifacts);
                }
                Err(e) => {
                    let _ = state_bg.task_store.fail(&task_id, &e);
                }
            },
            None => {
                let _ = state_bg.task_store.fail(&task_id, "No skill specified");
            }
        }
    });

    // Return the task in working state — client polls for completion
    let current_task = state.task_store.get(&task.id).unwrap_or(task);
    let response = TaskSendResponse {
        id: current_task.id.clone(),
        session_id: current_task.session_id.clone(),
        context_id: current_task.context_id.clone().unwrap_or_default(),
        status: current_task.status.clone(),
        artifacts: current_task.artifacts.clone(),
    };

    (
        StatusCode::OK,
        Json(serde_json::to_value(response).unwrap_or_default()),
    )
}

/// POST /a2a/tasks/sendSubscribe (streaming via SSE)
pub async fn tasks_send_subscribe_handler(
    State(state): State<A2aState>,
    Json(request): Json<TaskSendRequest>,
) -> impl IntoResponse {
    info!("A2A tasks/sendSubscribe (SSE): task_id={}", request.id);

    // Create task and event channel
    let task = state
        .task_store
        .create(&request.session_id, &request.message);
    let task_id = task.id.clone();
    let (tx, rx) = crate::internal::a2a::sse::task_event_channel(32);

    // Send "submitted" event
    crate::internal::a2a::sse::send_event(
        &tx,
        &task_id,
        TaskState::Submitted,
        Some("Task received"),
        None,
        false,
    )
    .await;

    state.task_store.start_working(&task_id).ok();

    // Send "working" event
    crate::internal::a2a::sse::send_event(
        &tx,
        &task_id,
        TaskState::Working,
        Some("Executing tool"),
        None,
        false,
    )
    .await;

    // Execute the tool in a spawned task, sending completion/failure via the channel
    let state_clone = state.clone();
    let tx_clone = tx.clone();
    let skill_name = extract_skill_name(&request.message);
    let params = extract_parameters(&request.message);

    tokio::spawn(async move {
        match skill_name {
            Some(skill) => match execute_tool(&state_clone, &task_id, &skill, &params).await {
                Ok(artifacts) => {
                    state_clone
                        .task_store
                        .complete(&task_id, artifacts.clone())
                        .ok();
                    crate::internal::a2a::sse::send_event(
                        &tx_clone,
                        &task_id,
                        TaskState::Completed,
                        Some("Task completed"),
                        Some(artifacts),
                        true,
                    )
                    .await;
                }
                Err(e) => {
                    state_clone.task_store.fail(&task_id, &e).ok();
                    crate::internal::a2a::sse::send_event(
                        &tx_clone,
                        &task_id,
                        TaskState::Failed,
                        Some(&e),
                        None,
                        true,
                    )
                    .await;
                }
            },
            None => {
                let err = "No skill specified in message".to_string();
                state_clone.task_store.fail(&task_id, &err).ok();
                crate::internal::a2a::sse::send_event(
                    &tx_clone,
                    &task_id,
                    TaskState::Failed,
                    Some(&err),
                    None,
                    true,
                )
                .await;
            }
        }
    });

    // Return SSE stream
    crate::internal::a2a::sse::receiver_to_sse(rx)
}

/// GET /a2a/tasks/get?id=xxx&sessionId=yyy
#[derive(Debug, Deserialize)]
pub struct TaskGetQueryParams {
    pub id: String,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

pub async fn tasks_get_handler(
    State(state): State<A2aState>,
    Query(params): Query<TaskGetQueryParams>,
) -> impl IntoResponse {
    info!("A2A tasks/get: id={}", params.id);

    match state.task_store.get(&params.id) {
        Some(task) => {
            let response = serde_json::json!({
                "id": task.id,
                "sessionId": task.session_id,
                "contextId": task.context_id,
                "status": task.status,
                "artifacts": task.artifacts,
                "history": task.history,
            });
            (StatusCode::OK, Json(response))
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Task {} not found", params.id)
            })),
        ),
    }
}

/// POST /a2a/tasks/cancel
pub async fn tasks_cancel_handler(
    State(state): State<A2aState>,
    Json(request): Json<TaskCancelRequest>,
) -> impl IntoResponse {
    info!("A2A tasks/cancel: id={}", request.id);

    match state.task_store.cancel(&request.id) {
        Ok(task) => {
            let response = serde_json::json!({
                "id": task.id,
                "sessionId": task.session_id,
                "status": task.status,
            });
            (StatusCode::OK, Json(response))
        }
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Extract the skill name from a message's parts
fn extract_skill_name(message: &TaskMessage) -> Option<String> {
    for part in &message.parts {
        match part {
            Part::Data { data } => {
                if let Some(skill) = data.get("skill").and_then(|s| s.as_str()) {
                    return Some(skill.to_string());
                }
            }
            Part::Text { text } => {
                // Fallback: try to parse text as JSON to find skill name
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(skill) = parsed.get("skill").and_then(|s| s.as_str()) {
                        return Some(skill.to_string());
                    }
                }
                // If text is just a simple string, treat it as the skill name
                if !text.contains(' ') && !text.contains('\n') {
                    return Some(text.clone());
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract parameters from a message's parts
fn extract_parameters(message: &TaskMessage) -> serde_json::Value {
    for part in &message.parts {
        match part {
            Part::Data { data } => {
                if let Some(params) = data.get("parameters") {
                    return params.clone();
                }
                // If no explicit "parameters" field, the whole data is the params
                // (minus the "skill" field)
                let mut params = data.clone();
                if let Some(obj) = params.as_object_mut() {
                    obj.remove("skill");
                }
                return params;
            }
            Part::Text { text } => {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(params) = parsed.get("parameters") {
                        return params.clone();
                    }
                }
            }
            _ => {}
        }
    }
    serde_json::json!({})
}

/// Execute a tool by name and return artifacts
async fn execute_tool(
    state: &A2aState,
    task_id: &str,
    skill_name: &str,
    params: &serde_json::Value,
) -> Result<Vec<Artifact>, String> {
    let tool = state
        .tool_registry
        .get(skill_name)
        .ok_or_else(|| format!("Skill '{}' not found in tool registry", skill_name))?;

    // Build an MCP CallToolRequestParams
    let mut call_request = rmcp::model::CallToolRequestParams::new(skill_name.to_string());
    if let Some(args) = params
        .as_object()
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    {
        call_request = call_request.with_arguments(args);
    }

    // Execute via the tool's executor (same code path as MCP tools/call)
    match (tool.executor)(call_request).await {
        Ok(result) => {
            // Convert MCP CallToolResult → A2A Artifact
            let artifact = Artifact {
                artifact_id: format!("{task_id}-result"),
                name: Some(skill_name.to_string()),
                description: None,
                parts: result
                    .content
                    .iter()
                    .map(|c| match c {
                        rmcp::model::ContentBlock::Text(t) => Part::Text {
                            text: t.text.clone(),
                        },
                        _ => Part::Text {
                            text: "Unsupported content type".into(),
                        },
                    })
                    .collect(),
                metadata: None,
            };

            Ok(vec![artifact])
        }
        Err(e) => Err(format!("Tool execution failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_skill_from_data_part() {
        let msg = TaskMessage {
            role: "user".into(),
            parts: vec![Part::Data {
                data: serde_json::json!({
                    "skill": "get_users",
                    "parameters": {"page": 1}
                }),
            }],
            metadata: None,
        };

        let skill = extract_skill_name(&msg);
        assert_eq!(skill, Some("get_users".to_string()));

        let params = extract_parameters(&msg);
        assert_eq!(params["page"], 1);
    }

    #[test]
    fn test_extract_skill_from_text_part() {
        let msg = TaskMessage {
            role: "user".into(),
            parts: vec![Part::Text {
                text: "listTodos".into(),
            }],
            metadata: None,
        };

        let skill = extract_skill_name(&msg);
        assert_eq!(skill, Some("listTodos".to_string()));
    }

    #[test]
    fn test_extract_skill_none() {
        let msg = TaskMessage {
            role: "user".into(),
            parts: vec![Part::Text {
                text: "Please list all users and their todos".into(),
            }],
            metadata: None,
        };

        let skill = extract_skill_name(&msg);
        assert_eq!(skill, None);
    }
}
