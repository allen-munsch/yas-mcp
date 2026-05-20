//! A2A Task Store
//!
//! Thread-safe in-memory task storage with TTL-based expiry.
//! Uses DashMap for concurrent access without global locks.

use crate::internal::a2a::types::*;
use dashmap::DashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// A stored task with its creation time for TTL tracking
struct StoredTask {
    task: Task,
    created_at: Instant,
}

/// Thread-safe task store with configurable capacity and TTL
pub struct TaskStore {
    tasks: DashMap<String, Arc<Mutex<StoredTask>>>,
    max_tasks: usize,
    ttl: Duration,
}

impl TaskStore {
    /// Create a new task store
    pub fn new(max_tasks: usize, ttl_seconds: u64) -> Self {
        Self {
            tasks: DashMap::with_capacity(max_tasks),
            max_tasks,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// Create a new task in `submitted` state
    pub fn create(&self, session_id: &str, message: &TaskMessage) -> Task {
        // Enforce capacity: remove oldest if full
        if self.tasks.len() >= self.max_tasks {
            self.prune_one_oldest();
        }

        let task_id = Uuid::new_v4().to_string();
        let context_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let task = Task {
            id: task_id.clone(),
            session_id: session_id.to_string(),
            context_id: Some(context_id),
            status: TaskStatus {
                state: TaskState::Submitted,
                message: Some(message.clone()),
                timestamp: now,
            },
            artifacts: Vec::new(),
            history: vec![TaskEvent {
                state: TaskState::Submitted,
                timestamp: now,
                message: Some("Task created".into()),
            }],
            metadata: None,
        };

        let stored = StoredTask {
            task: task.clone(),
            created_at: Instant::now(),
        };

        self.tasks.insert(task_id, Arc::new(Mutex::new(stored)));
        task
    }

    /// Get a task by ID
    pub fn get(&self, id: &str) -> Option<Task> {
        self.tasks.get(id).map(|entry| {
            let arc = entry.value().clone();
            let stored = arc.lock().unwrap();
            stored.task.clone()
        })
    }

    /// Transition a task to a new state
    pub fn transition(
        &self,
        id: &str,
        new_state: TaskState,
        message: Option<String>,
        artifacts: Option<Vec<Artifact>>,
    ) -> Result<Task, String> {
        let entry = self
            .tasks
            .get(id)
            .ok_or_else(|| format!("Task {} not found", id))?;

        let arc = entry.value().clone();
        let mut stored = arc.lock().unwrap();

        // Don't transition from terminal states
        if stored.task.status.state.is_terminal() {
            return Err(format!(
                "Cannot transition task {} from terminal state {:?}",
                id, stored.task.status.state
            ));
        }

        let now = chrono::Utc::now();
        stored.task.status = TaskStatus {
            state: new_state.clone(),
            message: None,
            timestamp: now,
        };
        stored.task.history.push(TaskEvent {
            state: new_state,
            timestamp: now,
            message,
        });

        if let Some(arts) = artifacts {
            stored.task.artifacts.extend(arts);
        }

        Ok(stored.task.clone())
    }

    /// Cancel a task
    pub fn cancel(&self, id: &str) -> Result<Task, String> {
        self.transition(id, TaskState::Canceled, Some("Task canceled by client".into()), None)
    }

    /// Mark a task as working
    pub fn start_working(&self, id: &str) -> Result<Task, String> {
        self.transition(
            id,
            TaskState::Working,
            Some("Task execution started".into()),
            None,
        )
    }

    /// Complete a task with results
    pub fn complete(&self, id: &str, artifacts: Vec<Artifact>) -> Result<Task, String> {
        self.transition(
            id,
            TaskState::Completed,
            Some("Task completed successfully".into()),
            Some(artifacts),
        )
    }

    /// Fail a task with error
    pub fn fail(&self, id: &str, error: &str) -> Result<Task, String> {
        self.transition(
            id,
            TaskState::Failed,
            Some(format!("Task failed: {error}")),
            None,
        )
    }

    /// Prune expired tasks (tasks older than TTL)
    pub fn prune_expired(&self) -> usize {
        let now = Instant::now();
        let mut removed = 0;

        self.tasks.retain(|_id, entry| {
            let stored = entry.lock().unwrap();
            let keep = now.duration_since(stored.created_at) < self.ttl;
            if !keep {
                removed += 1;
            }
            keep
        });

        removed
    }

    /// Remove the oldest task (for capacity enforcement)
    fn prune_one_oldest(&self) {
        let mut oldest_key: Option<String> = None;
        let mut oldest_time = Instant::now();

        for entry in &self.tasks {
            let stored = entry.value().lock().unwrap();
            if stored.created_at < oldest_time {
                oldest_time = stored.created_at;
                oldest_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            self.tasks.remove(&key);
        }
    }

    /// Get the total number of tasks
    pub fn count(&self) -> usize {
        self.tasks.len()
    }

    /// Get count of tasks in a specific state
    pub fn count_by_state(&self, state: &TaskState) -> usize {
        self.tasks
            .iter()
            .filter(|entry| {
                let arc = entry.value().clone();
                let stored = arc.lock().unwrap();
                &stored.task.status.state == state
            })
            .count()
    }
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new(100, 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::a2a::types::Part;

    fn make_test_message() -> TaskMessage {
        TaskMessage {
            role: "user".into(),
            parts: vec![Part::Text {
                text: "test".into(),
            }],
            metadata: None,
        }
    }

    #[test]
    fn test_create_task() {
        let store = TaskStore::default();
        let msg = make_test_message();
        let task = store.create("session-1", &msg);

        assert_eq!(task.session_id, "session-1");
        assert_eq!(task.status.state, TaskState::Submitted);
        assert!(task.context_id.is_some());
        assert_eq!(task.history.len(), 1);
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn test_task_lifecycle() {
        let store = TaskStore::default();
        let msg = make_test_message();
        let task = store.create("session-1", &msg);
        let task_id = task.id.clone();

        // Submitted → Working
        let task = store.start_working(&task_id).unwrap();
        assert_eq!(task.status.state, TaskState::Working);

        // Working → Completed
        let artifact = Artifact {
            artifact_id: "art-1".into(),
            name: Some("result".into()),
            description: None,
            parts: vec![Part::Text {
                text: "result data".into(),
            }],
            metadata: None,
        };
        let task = store.complete(&task_id, vec![artifact]).unwrap();
        assert_eq!(task.status.state, TaskState::Completed);
        assert_eq!(task.artifacts.len(), 1);
        assert_eq!(task.history.len(), 3);
    }

    #[test]
    fn test_cannot_transition_from_terminal() {
        let store = TaskStore::default();
        let msg = make_test_message();
        let task = store.create("session-1", &msg);
        let task_id = task.id.clone();

        store.complete(&task_id, vec![]).unwrap();
        let result = store.start_working(&task_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_cancel_task() {
        let store = TaskStore::default();
        let msg = make_test_message();
        let task = store.create("session-1", &msg);
        let task_id = task.id.clone();

        let task = store.cancel(&task_id).unwrap();
        assert_eq!(task.status.state, TaskState::Canceled);
    }

    #[test]
    fn test_prune_expired() {
        let store = TaskStore::new(10, 0); // TTL = 0 (instant expiry)
        let msg = make_test_message();
        store.create("session-1", &msg);

        let removed = store.prune_expired();
        assert_eq!(removed, 1);
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_count_by_state() {
        let store = TaskStore::default();
        let msg = make_test_message();

        let task1 = store.create("s1", &msg);
        let _task2 = store.create("s2", &msg);

        store.complete(&task1.id, vec![]).unwrap();
        // task2 still Submitted

        assert_eq!(store.count_by_state(&TaskState::Completed), 1);
        assert_eq!(store.count_by_state(&TaskState::Submitted), 1);
    }

    #[test]
    fn test_task_not_found() {
        let store = TaskStore::default();
        let result = store.get("nonexistent");
        assert!(result.is_none());

        let result = store.transition("nonexistent", TaskState::Working, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_fail_task() {
        let store = TaskStore::default();
        let msg = make_test_message();
        let task = store.create("session-1", &msg);

        let task = store.fail(&task.id, "something went wrong").unwrap();
        assert_eq!(task.status.state, TaskState::Failed);
        assert!(task.status.message.is_none());
        // History should record the failure
        assert!(
            task.history.iter().any(|e| e.state == TaskState::Failed),
            "History should contain Failed event"
        );
        assert!(
            task.history.iter().any(|e| e.message.as_deref() == Some("Task failed: something went wrong")),
            "History should contain error message"
        );
    }

    #[test]
    fn test_capacity_enforcement() {
        let store = TaskStore::new(3, 3600); // max 3 tasks
        let msg = make_test_message();

        store.create("s1", &msg);
        store.create("s2", &msg);
        store.create("s3", &msg);
        assert_eq!(store.count(), 3);

        // This should evict the oldest (s1)
        store.create("s4", &msg);
        assert_eq!(store.count(), 3, "Should still be at max capacity");
        assert!(store.get("s1").is_none(), "Oldest task should be evicted");
    }

    #[test]
    fn test_count_by_state_multiple() {
        let store = TaskStore::default();
        let msg = make_test_message();

        let t1 = store.create("s1", &msg);
        let t2 = store.create("s2", &msg);
        let _t3 = store.create("s3", &msg);

        store.complete(&t1.id, vec![]).unwrap();
        store.fail(&t2.id, "error").unwrap();
        // t3 remains Submitted

        assert_eq!(store.count_by_state(&TaskState::Completed), 1);
        assert_eq!(store.count_by_state(&TaskState::Failed), 1);
        assert_eq!(store.count_by_state(&TaskState::Submitted), 1);
    }

    #[test]
    fn test_complete_with_artifacts() {
        let store = TaskStore::default();
        let msg = make_test_message();
        let task = store.create("s1", &msg);

        let artifacts = vec![
            Artifact {
                artifact_id: "art-1".into(),
                name: Some("result.json".into()),
                description: Some("API response".into()),
                parts: vec![Part::Text {
                    text: "{\"ok\": true}".into(),
                }],
                metadata: None,
            },
            Artifact {
                artifact_id: "art-2".into(),
                name: Some("log.txt".into()),
                description: None,
                parts: vec![Part::Text {
                    text: "execution log".into(),
                }],
                metadata: None,
            },
        ];

        let task = store.complete(&task.id, artifacts).unwrap();
        assert_eq!(task.artifacts.len(), 2);
        assert_eq!(task.artifacts[0].name.as_deref(), Some("result.json"));
        assert_eq!(task.artifacts[1].name.as_deref(), Some("log.txt"));
    }
}
