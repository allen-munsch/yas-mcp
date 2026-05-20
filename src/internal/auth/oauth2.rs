use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2ProviderConfig {
    pub provider: String,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub user_info_url: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_uri: Option<String>,
    /// Provider-specific additional parameters
    pub extra_params: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Token {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub id_token: Option<String>, // For OpenID Connect
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub provider: String,
}

pub struct OAuth2Client {
    config: OAuth2ProviderConfig,
    client: Client,
}

impl OAuth2Client {
    pub fn new(config: OAuth2ProviderConfig) -> Result<Self> {
        let client = Client::new();
        Ok(Self { config, client })
    }

    /// Generate provider-specific authorization URL
    pub fn get_authorization_url(&self, state: &str) -> String {
        let scopes = self.config.scopes.join(" ");
        let redirect_uri = self
            .config
            .redirect_uri
            .as_deref()
            .unwrap_or("http://localhost:8080/oauth/callback");

        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            self.config.auth_url, self.config.client_id, redirect_uri, scopes, state
        );

        // Add provider-specific parameters
        if let Some(extra_params) = &self.config.extra_params {
            for (key, value) in extra_params {
                url.push_str(&format!("&{}={}", key, value));
            }
        }

        debug!("Generated OAuth2 URL for {}: {}", self.config.provider, url);
        url
    }

    /// Exchange authorization code for access token (provider-agnostic)
    pub async fn exchange_code(&self, code: &str) -> Result<OAuth2Token> {
        let params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("client_id", self.config.client_id.clone()),
            ("client_secret", self.config.client_secret.clone()),
            (
                "redirect_uri",
                self.config
                    .redirect_uri
                    .clone()
                    .unwrap_or("http://localhost:8080/oauth/callback".to_string()),
            ),
        ];

        debug!(
            "Exchanging OAuth2 code for {} at {}",
            self.config.provider, self.config.token_url
        );

        let response = self
            .client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                anyhow!(
                    "Failed to exchange OAuth2 code for {}: {}",
                    self.config.provider,
                    e
                )
            })?;

        self.handle_token_response(response).await
    }

    /// Handle token response (provider-specific parsing)
    async fn handle_token_response(&self, response: reqwest::Response) -> Result<OAuth2Token> {
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!(
                "OAuth2 token exchange failed for {}: {}",
                self.config.provider, error_text
            );
            return Err(anyhow!("OAuth2 token exchange failed: {}", status));
        }

        let token: OAuth2Token = response.json().await.map_err(|e| {
            anyhow!(
                "Failed to parse OAuth2 token response from {}: {}",
                self.config.provider,
                e
            )
        })?;

        info!(
            "Successfully obtained OAuth2 access token for {}",
            self.config.provider
        );
        Ok(token)
    }

    /// Get user information (provider-specific)
    pub async fn get_user_info(&self, access_token: &str) -> Result<UserInfo> {
        let user_info_url = match &self.config.user_info_url {
            Some(url) => url,
            None => {
                return Err(anyhow!(
                    "No user info URL configured for {}",
                    self.config.provider
                ))
            }
        };

        let response = self
            .client
            .get(user_info_url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("User-Agent", "yas-mcp") // Some providers require User-Agent
            .send()
            .await
            .map_err(|e| {
                anyhow!(
                    "Failed to get user info from {}: {}",
                    self.config.provider,
                    e
                )
            })?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to get user info: {}", response.status()));
        }

        self.parse_user_info_response(response).await
    }

    /// Parse user info response (provider-specific)
    async fn parse_user_info_response(&self, response: reqwest::Response) -> Result<UserInfo> {
        let user_data: serde_json::Value = response.json().await.map_err(|e| {
            anyhow!(
                "Failed to parse user info from {}: {}",
                self.config.provider,
                e
            )
        })?;

        // Provider-specific parsing
        match self.config.provider.to_lowercase().as_str() {
            "github" => self.parse_github_user_info(user_data),
            "google" => self.parse_google_user_info(user_data),
            "microsoft" => self.parse_microsoft_user_info(user_data),
            _ => self.parse_generic_user_info(user_data),
        }
    }

    /// GitHub-specific user info parsing
    fn parse_github_user_info(&self, data: serde_json::Value) -> Result<UserInfo> {
        Ok(UserInfo {
            id: data["id"].as_i64().unwrap_or(0).to_string(),
            email: data["email"].as_str().unwrap_or("").to_string(),
            name: data["name"].as_str().map(|s| s.to_string()),
            picture: data["avatar_url"].as_str().map(|s| s.to_string()),
            provider: "github".to_string(),
        })
    }

    /// Google-specific user info parsing
    fn parse_google_user_info(&self, data: serde_json::Value) -> Result<UserInfo> {
        Ok(UserInfo {
            id: data["sub"].as_str().unwrap_or("").to_string(),
            email: data["email"].as_str().unwrap_or("").to_string(),
            name: data["name"].as_str().map(|s| s.to_string()),
            picture: data["picture"].as_str().map(|s| s.to_string()),
            provider: "google".to_string(),
        })
    }

    /// Microsoft-specific user info parsing
    fn parse_microsoft_user_info(&self, data: serde_json::Value) -> Result<UserInfo> {
        Ok(UserInfo {
            id: data["id"].as_str().unwrap_or("").to_string(),
            email: data["mail"]
                .as_str()
                .or_else(|| data["userPrincipalName"].as_str())
                .unwrap_or("")
                .to_string(),
            name: data["displayName"].as_str().map(|s| s.to_string()),
            picture: None, // Microsoft Graph doesn't return profile picture by default
            provider: "microsoft".to_string(),
        })
    }

    /// Generic fallback user info parsing
    fn parse_generic_user_info(&self, data: serde_json::Value) -> Result<UserInfo> {
        // Try common field names
        let id = data["id"]
            .as_str()
            .or_else(|| data["sub"].as_str())
            .or_else(|| data["user_id"].as_str())
            .unwrap_or("unknown");

        let email = data["email"]
            .as_str()
            .or_else(|| data["mail"].as_str())
            .unwrap_or("");

        let name = data["name"]
            .as_str()
            .or_else(|| data["displayName"].as_str())
            .or_else(|| data["username"].as_str())
            .map(|s| s.to_string());

        let picture = data["picture"]
            .as_str()
            .or_else(|| data["avatar_url"].as_str())
            .or_else(|| data["photoURL"].as_str())
            .map(|s| s.to_string());

        Ok(UserInfo {
            id: id.to_string(),
            email: email.to_string(),
            name,
            picture,
            provider: self.config.provider.clone(),
        })
    }

    /// Refresh access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OAuth2Token> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
        ];

        let response = self
            .client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                anyhow!(
                    "Failed to refresh OAuth2 token for {}: {}",
                    self.config.provider,
                    e
                )
            })?;

        self.handle_token_response(response).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> OAuth2ProviderConfig {
        OAuth2ProviderConfig {
            provider: "test".into(),
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            auth_url: "https://test.example.com/auth".into(),
            token_url: "https://test.example.com/token".into(),
            user_info_url: Some("https://test.example.com/userinfo".into()),
            scopes: vec!["openid".into(), "profile".into()],
            redirect_uri: Some("http://localhost:3000/callback".into()),
            extra_params: None,
        }
    }

    #[test]
    fn test_new_client() {
        let config = make_config();
        let client = OAuth2Client::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_get_authorization_url() {
        let config = make_config();
        let client = OAuth2Client::new(config).unwrap();
        let url = client.get_authorization_url("state123");

        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=test-client"));
        assert!(url.contains("state=state123"));
        assert!(url.contains("scope=openid") && url.contains("profile"));
        assert!(url.contains("redirect_uri=") && url.contains("localhost") && url.contains("callback"));
    }

    #[test]
    fn test_get_authorization_url_with_extra_params() {
        let mut config = make_config();
        config.extra_params = Some({
            let mut m = HashMap::new();
            m.insert("prompt".into(), "consent".into());
            m
        });
        let client = OAuth2Client::new(config).unwrap();
        let url = client.get_authorization_url("state");
        assert!(url.contains("prompt=consent"));
    }

    #[test]
    fn test_enable_github_user_info() {
        let mut config = make_config();
        config.provider = "github".into();
        let client = OAuth2Client::new(config).unwrap();

        let data = serde_json::json!({
            "id": 12345,
            "email": "user@github.com",
            "name": "GitHub User",
            "avatar_url": "https://avatars.example.com/12345"
        });
        let info = client.parse_github_user_info(data).unwrap();
        assert_eq!(info.id, "12345");
        assert_eq!(info.email, "user@github.com");
        assert_eq!(info.name, Some("GitHub User".into()));
        assert_eq!(info.provider, "github");
    }

    #[test]
    fn test_parse_google_user_info() {
        let config = make_config();
        let client = OAuth2Client::new(config).unwrap();

        let data = serde_json::json!({
            "sub": "google-sub-123",
            "email": "user@gmail.com",
            "name": "Google User",
            "picture": "https://photos.example.com/user.jpg"
        });
        let info = client.parse_google_user_info(data).unwrap();
        assert_eq!(info.id, "google-sub-123");
        assert_eq!(info.email, "user@gmail.com");
        assert_eq!(info.name, Some("Google User".into()));
    }

    #[test]
    fn test_parse_microsoft_user_info() {
        let config = make_config();
        let client = OAuth2Client::new(config).unwrap();

        let data = serde_json::json!({
            "id": "ms-id-456",
            "mail": "user@outlook.com",
            "displayName": "MS User"
        });
        let info = client.parse_microsoft_user_info(data).unwrap();
        assert_eq!(info.id, "ms-id-456");
        assert_eq!(info.email, "user@outlook.com");
        assert_eq!(info.name, Some("MS User".into()));
    }

    #[test]
    fn test_parse_generic_user_info() {
        let mut config = make_config();
        config.provider = "custom".into();
        let client = OAuth2Client::new(config).unwrap();

        let data = serde_json::json!({
            "sub": "custom-sub",
            "email": "user@custom.com",
            "name": "Custom User"
        });
        let info = client.parse_generic_user_info(data).unwrap();
        assert_eq!(info.id, "custom-sub");
        assert_eq!(info.email, "user@custom.com");
        assert_eq!(info.provider, "custom");
    }

    #[test]
    fn test_oauth2_token_deserialization() {
        let json = serde_json::json!({
            "access_token": "tok_abc123",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "ref_xyz789",
            "scope": "openid profile",
            "id_token": "eyJhbGci..."
        });
        let token: OAuth2Token = serde_json::from_value(json).unwrap();
        assert_eq!(token.access_token, "tok_abc123");
        assert_eq!(token.token_type, "Bearer");
        assert_eq!(token.expires_in, Some(3600));
        assert_eq!(token.refresh_token, Some("ref_xyz789".into()));
    }
}
