//! A2A SSE Streaming
//!
//! Server-Sent Events transport for `tasks/sendSubscribe`.
//! Streams task lifecycle events: submitted → working → completed/failed.
//!
//! # Architecture
//!
//! ```text
//! Client                    yas-mcp                   Tool Executor
//!   │                          │                          │
//!   │ POST /a2a/tasks/         │                          │
//!   │   sendSubscribe          │                          │
//!   │─────────────────────────►│                          │
//!   │                          │ create task              │
//!   │◄─ SSE: submitted ────────│                          │
//!   │◄─ SSE: working ──────────│                          │
//!   │                          │──── execute tool ───────►│
//!   │                          │◄─── result ──────────────│
//!   │◄─ SSE: completed ────────│                          │
//!   │                          │                          │
//! ```

use crate::internal::a2a::types::*;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio::sync::mpsc;
use tracing::debug;

/// An event in the SSE stream for a task
#[derive(Debug, Clone)]
pub struct TaskStreamEvent {
    /// Task ID this event belongs to
    pub task_id: String,
    /// Current task state
    pub state: TaskState,
    /// Optional message
    pub message: Option<String>,
    /// Optional artifacts (only on completion)
    pub artifacts: Option<Vec<Artifact>>,
    /// Whether this is the final event
    pub is_final: bool,
}

impl TaskStreamEvent {
    /// Convert to a JSON string for SSE
    pub fn to_sse_json(&self) -> String {
        let payload = serde_json::json!({
            "id": self.task_id,
            "status": {
                "state": self.state,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            },
            "message": self.message,
            "artifacts": self.artifacts,
            "final": self.is_final,
        });
        serde_json::to_string(&payload).unwrap_or_default()
    }
}

/// A channel sender for task stream events
pub type TaskEventSender = mpsc::Sender<TaskStreamEvent>;
/// A channel receiver for task stream events
pub type TaskEventReceiver = mpsc::Receiver<TaskStreamEvent>;

/// Create a new channel pair for task events
pub fn task_event_channel(buffer: usize) -> (TaskEventSender, TaskEventReceiver) {
    mpsc::channel(buffer)
}

/// Convert a receiver into an SSE stream suitable for axum
pub fn receiver_to_sse(
    mut receiver: TaskEventReceiver,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        while let Some(event) = receiver.recv().await {
            let event_type = match event.state {
                TaskState::Submitted => "submitted",
                TaskState::Working => "working",
                TaskState::InputRequired => "input-required",
                TaskState::Completed => "completed",
                TaskState::Failed => "failed",
                TaskState::Canceled => "canceled",
                TaskState::Rejected => "rejected",
            };

            let data = event.to_sse_json();
            debug!("SSE event: {} for task {}", event_type, event.task_id);

            yield Ok(Event::default()
                .event(event_type)
                .data(data));

            if event.is_final {
                break;
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Send a task state event through the channel
pub async fn send_event(
    sender: &TaskEventSender,
    task_id: &str,
    state: TaskState,
    message: Option<&str>,
    artifacts: Option<Vec<Artifact>>,
    is_final: bool,
) {
    let event = TaskStreamEvent {
        task_id: task_id.to_string(),
        state,
        message: message.map(|s| s.to_string()),
        artifacts,
        is_final,
    };

    let _ = sender.send(event).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_event_channel() {
        let (tx, mut rx) = task_event_channel(16);

        send_event(
            &tx,
            "task-1",
            TaskState::Submitted,
            Some("Task created"),
            None,
            false,
        )
        .await;

        send_event(
            &tx,
            "task-1",
            TaskState::Completed,
            Some("Done"),
            Some(vec![]),
            true,
        )
        .await;

        drop(tx); // Close sender

        let event1 = rx.recv().await.unwrap();
        assert_eq!(event1.task_id, "task-1");
        assert_eq!(event1.state, TaskState::Submitted);
        assert!(!event1.is_final);

        let event2 = rx.recv().await.unwrap();
        assert_eq!(event2.state, TaskState::Completed);
        assert!(event2.is_final);

        // Channel should be closed after final event + drop
        assert!(rx.recv().await.is_none());
    }

    #[test]
    fn test_event_to_sse_json() {
        let event = TaskStreamEvent {
            task_id: "task-abc".into(),
            state: TaskState::Working,
            message: Some("Processing...".into()),
            artifacts: None,
            is_final: false,
        };

        let json = event.to_sse_json();
        assert!(json.contains("task-abc"));
        assert!(json.contains("working"));
        assert!(json.contains("Processing..."));
    }

    #[test]
    fn test_final_event_to_sse_json() {
        let event = TaskStreamEvent {
            task_id: "task-xyz".into(),
            state: TaskState::Completed,
            message: Some("All done".into()),
            artifacts: Some(vec![Artifact {
                artifact_id: "art-1".into(),
                name: Some("result".into()),
                description: None,
                parts: vec![Part::Text {
                    text: "data".into(),
                }],
                metadata: None,
            }]),
            is_final: true,
        };

        let json = event.to_sse_json();
        assert!(json.contains("\"final\":true"));
        assert!(json.contains("artifacts"));
    }
}
