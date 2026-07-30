//! Record/Replay — Offline API Response Cache
//!
//! Record real API responses once, replay them forever.
//! Never hit the upstream API again.
//!
//! # Modes
//!
//! | Flag | Behavior |
//! |------|----------|
//! | (none) | Normal mode — calls upstream API |
//! | `--record` | Calls upstream AND saves response to disk |
//! | `--replay` | Returns from disk, never calls upstream |
//!
//! # Storage
//!
//! Recordings are stored as JSON files in `{dir}/{tool_name}/{hash}.json`.
//! The hash is SHA256 of `{method}:{path}:{params_json}`.
//!
//! ```text
//! recordings/
//! ├── get__projects/
//! │   ├── a1b2c3.json       # get__projects with params {"page":1}
//! │   └── d4e5f6.json       # get__projects with params {"page":2}
//! └── get__users_me/
//!     └── 1a2b3c.json        # get__users_me with params {}
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tracing::{debug, info, warn};

/// A single recorded API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    /// HTTP status code
    pub status: u16,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body (as bytes, base64-encoded in JSON)
    #[serde(with = "base64_body")]
    pub body: Vec<u8>,
    /// When this recording was made
    pub recorded_at: String,
    /// The tool name that produced this recording
    pub tool: String,
    /// The request parameters that produced this recording
    pub params_hash: String,
}

mod base64_body {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = STANDARD.encode(bytes);
        encoded.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        STANDARD.decode(&encoded).map_err(serde::de::Error::custom)
    }
}

/// Configuration for the record/replay engine
#[derive(Debug, Clone)]
pub struct RecordReplayConfig {
    /// If true, record responses to disk
    pub record: bool,
    /// If true, replay from disk instead of calling upstream
    pub replay: bool,
    /// Directory to store/read recordings
    pub directory: PathBuf,
}

impl Default for RecordReplayConfig {
    fn default() -> Self {
        Self {
            record: false,
            replay: false,
            directory: PathBuf::from("recordings"),
        }
    }
}

/// The record/replay engine
#[derive(Debug)]
pub struct RecordReplay {
    config: RecordReplayConfig,
    /// In-memory cache of loaded recordings (for replay mode)
    cache: RwLock<HashMap<String, Recording>>,
}

impl RecordReplay {
    /// Create a new record/replay engine
    pub fn new(config: RecordReplayConfig) -> Result<Self> {
        if config.record && config.replay {
            return Err(anyhow::anyhow!(
                "Cannot enable both --record and --replay at the same time"
            ));
        }

        // Create directory if recording
        if config.record {
            std::fs::create_dir_all(&config.directory)
                .context("Failed to create recordings directory")?;
            info!(
                "📼 Record mode — saving responses to {}",
                config.directory.display()
            );
        }

        // Pre-load recordings into memory if replaying
        let cache = if config.replay {
            info!(
                "📼 Replay mode — serving from {}",
                config.directory.display()
            );
            let mut cache = HashMap::new();
            if config.directory.exists() {
                Self::load_all(&config.directory, &mut cache)?;
            } else {
                warn!(
                    "Replay directory {} does not exist — no recordings available",
                    config.directory.display()
                );
            }
            cache
        } else {
            HashMap::new()
        };

        Ok(Self {
            config,
            cache: RwLock::new(cache),
        })
    }

    /// Check if a recording exists (replay mode)
    pub fn lookup(
        &self,
        tool: &str,
        method: &str,
        path: &str,
        params_json: &str,
    ) -> Option<Recording> {
        if !self.config.replay {
            return None;
        }

        let hash = Self::make_hash(method, path, params_json);
        let key = format!("{tool}:{hash}");

        let cache = self.cache.read().unwrap();
        cache.get(&key).cloned()
    }

    /// Save a recording (record mode)
    pub fn save(
        &self,
        tool: &str,
        method: &str,
        path: &str,
        params_json: &str,
        status: u16,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<()> {
        if !self.config.record {
            return Ok(());
        }

        let hash = Self::make_hash(method, path, params_json);
        let recording = Recording {
            status,
            headers: headers.clone(),
            body: body.to_vec(),
            recorded_at: chrono::Utc::now().to_rfc3339(),
            tool: tool.to_string(),
            params_hash: hash.clone(),
        };

        // Ensure tool directory exists
        let tool_dir = self.config.directory.join(tool);
        std::fs::create_dir_all(&tool_dir)?;

        // Write recording
        let file_path = tool_dir.join(format!("{hash}.json"));
        let json = serde_json::to_string_pretty(&recording)?;
        std::fs::write(&file_path, &json)?;

        debug!("📼 Recorded: {} → {}", tool, file_path.display());

        // Also cache in memory if in replay mode
        if self.config.replay {
            let key = format!("{tool}:{hash}");
            self.cache.write().unwrap().insert(key, recording);
        }

        Ok(())
    }

    /// Number of recordings loaded (replay mode)
    pub fn recording_count(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    /// Make a deterministic hash from request parameters
    fn make_hash(method: &str, path: &str, params_json: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(method.as_bytes());
        hasher.update(b":");
        hasher.update(path.as_bytes());
        hasher.update(b":");
        hasher.update(params_json.as_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)[..16].to_string()
    }

    /// Load all recordings from a directory into the cache
    fn load_all(dir: &Path, cache: &mut HashMap<String, Recording>) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let tool_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");

                for file_entry in std::fs::read_dir(&path)? {
                    let file_entry = file_entry?;
                    let file_path = file_entry.path();

                    if file_path.extension().map_or(false, |e| e == "json") {
                        match std::fs::read_to_string(&file_path) {
                            Ok(json) => match serde_json::from_str::<Recording>(&json) {
                                Ok(recording) => {
                                    let key = format!("{}:{}", tool_name, recording.params_hash);
                                    debug!(
                                        "📼 Loaded recording: {} ({} bytes)",
                                        file_path.display(),
                                        recording.body.len()
                                    );
                                    cache.insert(key, recording);
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to parse recording {}: {}",
                                        file_path.display(),
                                        e
                                    );
                                }
                            },
                            Err(e) => {
                                warn!("Failed to read recording {}: {}", file_path.display(), e);
                            }
                        }
                    }
                }
            }
        }

        info!(
            "📼 Loaded {} recordings from {}",
            cache.len(),
            dir.display()
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(record: bool, replay: bool) -> RecordReplayConfig {
        let dir = std::env::temp_dir().join(format!("yas-mcp-test-{}", uuid::Uuid::new_v4()));
        RecordReplayConfig {
            record,
            replay,
            directory: dir,
        }
    }

    #[test]
    fn test_record_and_replay() {
        let config = make_config(true, false);
        let rr = RecordReplay::new(config.clone()).unwrap();

        // Record a response
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "application/json".into());
        rr.save("get_users", "GET", "/users", "{}", 200, &headers, b"hello")
            .unwrap();

        // Verify file exists
        let hash = RecordReplay::make_hash("GET", "/users", "{}");
        let file_path = config
            .directory
            .join("get_users")
            .join(format!("{hash}.json"));
        assert!(file_path.exists());

        // Now replay
        let replay_config = make_config(false, true);
        // Copy the file to the replay directory
        let replay_dir = replay_config.directory.clone();
        std::fs::create_dir_all(&replay_dir).unwrap();
        let src = config.directory.clone();
        // Copy recorded files to replay dir
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                let tool = entry.file_name();
                let dest = replay_dir.join(&tool);
                std::fs::create_dir_all(&dest).unwrap();
                for file in std::fs::read_dir(entry.path()).unwrap() {
                    let file = file.unwrap();
                    std::fs::copy(file.path(), dest.join(file.file_name())).unwrap();
                }
            }
        }

        let rr_replay = RecordReplay::new(replay_config).unwrap();
        assert_eq!(rr_replay.recording_count(), 1);

        let found = rr_replay.lookup("get_users", "GET", "/users", "{}");
        assert!(found.is_some());
        let rec = found.unwrap();
        assert_eq!(rec.status, 200);
        assert_eq!(rec.body, b"hello");
        assert_eq!(rec.headers.get("content-type").unwrap(), "application/json");

        // Cleanup
        let _ = std::fs::remove_dir_all(&config.directory);
        let _ = std::fs::remove_dir_all(&rr_replay.config.directory);
    }

    #[test]
    fn test_cannot_enable_both_modes() {
        let mut config = make_config(false, false);
        config.record = true;
        config.replay = true;
        let result = RecordReplay::new(config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Cannot enable both")
        );
    }

    #[test]
    fn test_make_hash_deterministic() {
        let h1 = RecordReplay::make_hash("GET", "/users", "{}");
        let h2 = RecordReplay::make_hash("GET", "/users", "{}");
        assert_eq!(h1, h2, "Same inputs should produce same hash");
    }

    #[test]
    fn test_make_hash_different_params() {
        let h1 = RecordReplay::make_hash("GET", "/users", r#"{"page":1}"#);
        let h2 = RecordReplay::make_hash("GET", "/users", r#"{"page":2}"#);
        assert_ne!(h1, h2, "Different params should produce different hash");
    }

    #[test]
    fn test_make_hash_different_methods() {
        let h1 = RecordReplay::make_hash("GET", "/users", "{}");
        let h2 = RecordReplay::make_hash("POST", "/users", "{}");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_lookup_nonexistent() {
        let config = make_config(false, true);
        std::fs::create_dir_all(&config.directory).unwrap();
        let rr = RecordReplay::new(config.clone()).unwrap();

        let found = rr.lookup("nonexistent", "GET", "/", "{}");
        assert!(found.is_none());

        let _ = std::fs::remove_dir_all(&config.directory);
    }

    #[test]
    fn test_recording_serialization() {
        let mut headers = HashMap::new();
        headers.insert("x-custom".into(), "value".into());

        let rec = Recording {
            status: 200,
            headers,
            body: b"test body".to_vec(),
            recorded_at: "2025-01-01T00:00:00Z".into(),
            tool: "get_users".into(),
            params_hash: "abc123".into(),
        };

        let json = serde_json::to_string(&rec).unwrap();
        let parsed: Recording = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.status, 200);
        assert_eq!(parsed.body, b"test body");
        assert_eq!(parsed.tool, "get_users");
    }

    #[test]
    fn test_save_multiple_params() {
        let config = make_config(true, false);
        let rr = RecordReplay::new(config.clone()).unwrap();
        let headers = HashMap::new();

        rr.save(
            "get_users",
            "GET",
            "/users",
            r#"{"page":1}"#,
            200,
            &headers,
            b"page1",
        )
        .unwrap();
        rr.save(
            "get_users",
            "GET",
            "/users",
            r#"{"page":2}"#,
            200,
            &headers,
            b"page2",
        )
        .unwrap();

        // Verify two different files
        let tool_dir = config.directory.join("get_users");
        let count = std::fs::read_dir(&tool_dir).unwrap().count();
        assert_eq!(count, 2, "Should have 2 recordings for different params");

        let _ = std::fs::remove_dir_all(&config.directory);
    }

    #[test]
    fn test_empty_replay_directory() {
        let config = make_config(false, true);
        std::fs::create_dir_all(&config.directory).unwrap();
        let rr = RecordReplay::new(config.clone()).unwrap();
        assert_eq!(rr.recording_count(), 0);

        let _ = std::fs::remove_dir_all(&config.directory);
    }
}
