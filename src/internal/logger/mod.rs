// src/internal/logger/mod.rs

pub mod logger;

// Export the init_logger function
pub use logger::init_logger;

// Note: The log_* macros are exported at the crate root via #[macro_export]
// They can be accessed directly as crate::log_debug!, crate::log_info!, etc.
// or with `use crate::{log_debug, log_info, log_warn, log_error};`