pub mod oauth2;
// pub mod providers;  // Comment out for now if not implemented

use crate::internal::config::config::OAuthConfig;
use anyhow::{Result, anyhow};

/// Create provider-specific OAuth2 configuration
pub fn create_provider_config(config: &OAuthConfig) -> Result<oauth2::OAuth2ProviderConfig> {
    match config.provider.to_lowercase().as_str() {
        "github" => Ok(oauth2::OAuth2ProviderConfig {
            provider: "github".to_string(),
            auth_url: "https://github.com/oauth/authorize".to_string(),
            token_url: "https://github.com/oauth/access_token".to_string(),
            user_info_url: Some("https://api.github.com/user".to_string()),
            scopes: config.scopes.clone(),
            redirect_uri: config.redirect_uri.clone(),
            client_id: config.client_id.clone(),
            client_secret: config.client_secret.clone(),
            extra_params: Some({
                let mut params = std::collections::HashMap::new();
                params.insert("allow_signup".to_string(), "true".to_string());
                params
            }),
        }),
        "google" => Ok(oauth2::OAuth2ProviderConfig {
            provider: "google".to_string(),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            user_info_url: Some("https://www.googleapis.com/oauth2/v3/userinfo".to_string()),
            scopes: config.scopes.clone(),
            redirect_uri: config.redirect_uri.clone(),
            client_id: config.client_id.clone(),
            client_secret: config.client_secret.clone(),
            extra_params: Some({
                let mut params = std::collections::HashMap::new();
                params.insert("access_type".to_string(), "offline".to_string());
                params.insert("prompt".to_string(), "consent".to_string());
                params
            }),
        }),
        "microsoft" => Ok(oauth2::OAuth2ProviderConfig {
            provider: "microsoft".to_string(),
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string(),
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
            user_info_url: Some("https://graph.microsoft.com/v1.0/me".to_string()),
            scopes: config.scopes.clone(),
            redirect_uri: config.redirect_uri.clone(),
            client_id: config.client_id.clone(),
            client_secret: config.client_secret.clone(),
            extra_params: None,
        }),
        "generic" => Ok(oauth2::OAuth2ProviderConfig {
            provider: config.provider.clone(),
            auth_url: config.auth_url.clone().unwrap_or_default(),
            token_url: config.token_url.clone().unwrap_or_default(),
            user_info_url: config.user_info_url.clone(),
            scopes: config.scopes.clone(),
            redirect_uri: config.redirect_uri.clone(),
            client_id: config.client_id.clone(),
            client_secret: config.client_secret.clone(),
            extra_params: config.extra_params.clone(),
        }),
        _ => Err(anyhow!("Unsupported OAuth2 provider: {}", config.provider)),
    }
}