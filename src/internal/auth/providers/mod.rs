pub mod github;
// pub mod google;  // Comment out for now

// Remove async trait for now
/*
use async_trait::async_trait;
use crate::internal::auth::{OAuthToken, OAuthUser, OAuthConfig};

#[async_trait]
pub trait OAuthProvider: Send + Sync {
    async fn get_auth_url(&self, state: &str) -> Result<String, anyhow::Error>;
    async fn exchange_code(&self, code: &str) -> Result<OAuthToken, anyhow::Error>;
    async fn get_user_info(&self, token: &str) -> Result<OAuthUser, anyhow::Error>;
}

pub fn create_provider(config: &OAuthConfig) -> Result<Box<dyn OAuthProvider>, anyhow::Error> {
    match config.provider.as_str() {
        "github" => Ok(Box::new(github::GitHubProvider::new(config)?)),
        "google" => Ok(Box::new(google::GoogleProvider::new(config)?)),
        _ => Err(anyhow::anyhow!("Unsupported OAuth provider: {}", config.provider)),
    }
}
*/