pub mod handlers;
pub mod jwks;
pub mod middleware;
pub mod models;
pub mod oauth2;
pub mod oidc_discovery;
pub mod provider;
pub mod token_store;
pub mod wimse;
// pub mod providers;  // Comment out for now if not implemented

use crate::internal::config::OAuthConfig;
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
        "oidc" => {
            // OIDC with discovery — placeholder; real config via create_oidc_provider_config_async
            Ok(oauth2::OAuth2ProviderConfig {
                provider: "oidc".to_string(),
                auth_url: String::new(),
                token_url: String::new(),
                user_info_url: None,
                scopes: config.scopes.clone(),
                redirect_uri: config.redirect_uri.clone(),
                client_id: config.client_id.clone(),
                client_secret: config.client_secret.clone(),
                extra_params: config.extra_params.clone(),
            })
        }
        _ => Err(anyhow!("Unsupported OAuth2 provider: {}", config.provider)),
    }
}

/// Create an OAuth2 provider config using OIDC Discovery.
/// Fetches `.well-known/openid-configuration` from the issuer URL.
pub async fn create_oidc_provider_config_async(
    config: &OAuthConfig,
) -> Result<oauth2::OAuth2ProviderConfig> {
    let issuer_url = config
        .issuer_url
        .as_ref()
        .ok_or_else(|| anyhow!("issuer_url is required for OIDC provider"))?;

    let cached = oidc_discovery::OidcDiscovery::fetch_cached(
        issuer_url,
        std::time::Duration::from_secs(config.oidc_cache_ttl),
    )
    .await?;

    let doc = &cached.document;

    Ok(oauth2::OAuth2ProviderConfig {
        provider: "oidc".to_string(),
        auth_url: doc.authorization_endpoint.clone(),
        token_url: doc.token_endpoint.clone(),
        user_info_url: doc.userinfo_endpoint.clone(),
        scopes: config.scopes.clone(),
        redirect_uri: config.redirect_uri.clone(),
        client_id: config.client_id.clone(),
        client_secret: config.client_secret.clone(),
        extra_params: config.extra_params.clone(),
    })
}
