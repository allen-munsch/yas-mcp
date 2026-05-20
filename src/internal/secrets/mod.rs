//! Secrets Management
//!
//! Pluggable secret resolution system. Never put raw secrets in config files.
//!
//! # Secret References
//!
//! Use URI-like references in any config value:
//!
//! ```yaml
//! oauth:
//!   client_secret: "env://OAUTH_CLIENT_SECRET"
//!   # or
//!   client_secret: "file:///run/secrets/oauth-client-secret"
//!   # or
//!   client_secret: "vault://secret/data/yas-mcp#client_secret"
//! ```
//!
//! # Adding a custom resolver
//!
//! Implement [`SecretResolver`] and register it with [`SecretStore::register`]:
//!
//! ```rust,ignore
//! use yas_mcp::internal::secrets::{SecretStore, SecretResolver};
//! use std::sync::Arc;
//!
//! struct MyVaultResolver { ... }
//! impl SecretResolver for MyVaultResolver { ... }
//!
//! let mut store = SecretStore::default();
//! store.register(Arc::new(MyVaultResolver::new(...)));
//! ```

pub mod resolver;
pub mod store;

pub use resolver::SecretResolver;
pub use store::SecretStore;
