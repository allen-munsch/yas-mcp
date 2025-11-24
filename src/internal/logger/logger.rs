// src/internal/logger/logger.rs

use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;
use tracing_subscriber::{
    fmt::{self},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,  // Directly import EnvFilter
};

use crate::internal::config::LoggingConfig;

/// Initialize the global logger with the given configuration
pub fn init_logger(cfg: &LoggingConfig) -> anyhow::Result<()> {
    // Build filter using EnvFilter (no feature flags needed)
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(cfg.level.clone()));

    // Warn if JSON format is requested (requires feature)
    if cfg.format == "json" {
        eprintln!("Warning: JSON format requires the 'json' feature in tracing-subscriber. Using default format.");
    }

    // Build the subscriber based on configuration
    match (&cfg.output_path, cfg.disable_console) {
        // Both console and file
        (Some(output_path), false) => {
            let log_file = create_log_file(output_path, cfg.append_to_file)?;
            let file_writer = NonBlockingFileWriter::new(log_file);
            
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_ansi(cfg.color)
                        .with_level(true)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_thread_names(false)
                )
                .with(
                    fmt::layer()
                        .with_writer(file_writer)
                        .with_ansi(false)
                        .with_level(true)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_thread_names(false)
                )
                .init();
        }
        // Only file
        (Some(output_path), true) => {
            let log_file = create_log_file(output_path, cfg.append_to_file)?;
            let file_writer = NonBlockingFileWriter::new(log_file);
            
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_writer(file_writer)
                        .with_ansi(false)
                        .with_level(true)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_thread_names(false)
                )
                .init();
        }
        // Only console
        (None, false) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_ansi(cfg.color)
                        .with_level(true)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_thread_names(false)
                )
                .init();
        }
        // No output (shouldn't happen, but handle it)
        (None, true) => {
            tracing_subscriber::registry()
                .with(filter)
                .init();
        }
    }

    Ok(())
}

/// Create or open log file based on configuration
fn create_log_file(path: &str, append: bool) -> anyhow::Result<fs::File> {
    let path = Path::new(path);
    
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Remove existing file if append is disabled
    if !append && path.exists() {
        fs::remove_file(path)?;
    }

    let file = OpenOptions::new()
        .create(true)
        .append(append)
        .write(true)
        .open(path)?;

    Ok(file)
}

/// Custom writer for file logging with thread safety
#[derive(Clone)]
struct NonBlockingFileWriter {
    file: std::sync::Arc<std::sync::Mutex<fs::File>>,
}

impl NonBlockingFileWriter {
    fn new(file: fs::File) -> Self {
        Self {
            file: std::sync::Arc::new(std::sync::Mutex::new(file)),
        }
    }
}

impl io::Write for NonBlockingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.lock().unwrap().flush()
    }
}

impl<'a> fmt::MakeWriter<'a> for NonBlockingFileWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

// Remove the unused imports at the bottom and keep only the macros
// Convenience logging macros
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        tracing::debug!($($arg)*)
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        tracing::info!($($arg)*)
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        tracing::warn!($($arg)*)
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        tracing::error!($($arg)*)
    };
}