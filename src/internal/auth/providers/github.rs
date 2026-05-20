// Remove async trait for now
/*
use async_trait::async_trait;
use serde::Deserialize;
use crate::internal::auth::{OAuthConfig, OAuthToken, OAuthUser, OAuthProvider};

#[derive(Debug, Clone)]
pub struct GitHubProvider {
    config: OAuthConfig,
    client: reqwest::Client,
}

impl GitHubProvider {
    pub fn new(config: &OAuthConfig) -> Result<Self, anyhow::Error> {
        Ok(Self {
            config: config.clone(),
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl OAuthProvider for GitHubProvider {
    async fn get_auth_url(&self, state: &str) -> Result<String, anyhow::Error> {
        let scopes = self.config.scopes.join(" ");
        let url = format!(
            "https://github.com/oauth/authorize?client_id={}&scope={}&state={}",
            self.config.client_id, scopes, state
        );
        Ok(url)
    }

    async fn exchange_code(&self, code: &str) -> Result<OAuthToken, anyhow::Error> {
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("code", code),
        ];

        let response = self.client
            .post("https://github.com/oauth/access_token")
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await?;

        let token: GitHubTokenResponse = response.json().await?;
        Ok(OAuthToken {
            access_token: token.access_token,
            token_type: token.token_type,
            expires_in: None,
            refresh_token: None,
            scope: Some(token.scope),
        })
    }

    async fn get_user_info(&self, token: &str) -> Result<OAuthUser, anyhow::Error> {
        let response = self.client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", "yas-mcp")
            .send()
            .await?;

        let user: GitHubUser = response.json().await?;
        Ok(OAuthUser {
            id: user.id.to_string(),
            email: user.email.unwrap_or_default(),
            name: Some(user.name),
            avatar: Some(user.avatar_url),
        })
    }
}

#[derive(Debug, Deserialize)]
struct GitHubTokenResponse {
    access_token: String,
    token_type: String,
    scope: String,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    id: u64,
    login: String,
    name: String,
    email: Option<String>,
    avatar_url: String,
}
*/