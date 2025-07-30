use anyhow::{Result, anyhow};
use oauth2::{
    AuthUrl, ClientId, RedirectUrl, TokenUrl, TokenResponse,
    RefreshToken, Scope, CsrfToken, PkceCodeChallenge,
};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn, error};
use url::Url;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
    pub token_type: String,
}

pub struct AuthManager {
    config: Arc<Config>,
    oauth_client: BasicClient,
    tokens: Option<TokenData>,
}

impl AuthManager {
    pub fn new(config: Arc<Config>) -> Result<Self> {
        let client = BasicClient::new(
            ClientId::new(config.client_id.clone()),
            None, // No client secret for public clients
            AuthUrl::new("https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string())?,
            Some(TokenUrl::new("https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string())?),
        )
        .set_redirect_uri(RedirectUrl::new(config.redirect_uri.clone())?);

        let mut auth_manager = Self {
            config: config.clone(),
            oauth_client: client,
            tokens: None,
        };

        // Load existing tokens
        auth_manager.load_tokens()?;

        Ok(auth_manager)
    }

    fn load_tokens(&mut self) -> Result<()> {
        if self.config.token_file.exists() {
            match fs::read_to_string(&self.config.token_file) {
                Ok(content) => {
                    match serde_json::from_str::<TokenData>(&content) {
                        Ok(tokens) => {
                            self.tokens = Some(tokens);
                            info!("Tokens loaded from file");
                        }
                        Err(e) => {
                            warn!("Failed to parse token file: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read token file: {}", e);
                }
            }
        }
        Ok(())
    }

    fn save_tokens(&self) -> Result<()> {
        if let Some(ref tokens) = self.tokens {
            let content = serde_json::to_string_pretty(tokens)?;
            fs::write(&self.config.token_file, content)?;
            info!("Tokens saved to file");
        }
        Ok(())
    }

    pub fn is_authenticated(&self) -> bool {
        if let Some(ref tokens) = self.tokens {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            // Check if token is still valid (with 5 minute buffer)
            tokens.expires_at > now + 300
        } else {
            false
        }
    }

    pub async fn get_access_token(&mut self) -> Result<String> {
        if self.is_authenticated() {
            return Ok(self.tokens.as_ref().unwrap().access_token.clone());
        }

        // Try to refresh token
        if let Some(ref tokens) = self.tokens {
            if let Some(ref refresh_token) = tokens.refresh_token {
                if self.refresh_access_token(refresh_token.clone()).await.is_ok() {
                    return Ok(self.tokens.as_ref().unwrap().access_token.clone());
                }
            }
        }

        Err(anyhow!("Not authenticated and cannot refresh token"))
    }

    pub async fn authenticate(&mut self) -> Result<()> {
        info!("Starting authentication flow");

        // Generate PKCE challenge
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Build authorization URL
        let (auth_url, csrf_token) = self
            .oauth_client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("https://graph.microsoft.com/Files.ReadWrite.All".to_string()))
            .add_scope(Scope::new("https://graph.microsoft.com/User.Read".to_string()))
            .add_scope(Scope::new("offline_access".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        info!("Opening browser for authentication");
        if let Err(e) = open::that(auth_url.to_string()) {
            error!("Failed to open browser: {}", e);
            return Err(anyhow!("Failed to open browser for authentication"));
        }

        // Start local server to receive callback
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        info!("Callback server listening on http://127.0.0.1:8080");

        // Wait for callback
        let (mut stream, _) = listener.accept().await?;
        let mut reader = BufReader::new(&mut stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).await?;

        info!("Received callback request: {}", request_line.trim());

        // Parse the request to extract the authorization code
        let request_parts: Vec<&str> = request_line.split_whitespace().collect();
        if request_parts.len() < 2 {
            return Err(anyhow!("Invalid HTTP request format"));
        }
        
        let method = request_parts[0];
        let path = request_parts[1];
        
        info!("HTTP Method: {}, Path: {}", method, path);

        // Ensure it's a GET request to the callback path
        if method != "GET" {
            return Err(anyhow!("Expected GET request, got: {}", method));
        }

        let url = Url::parse(&format!("http://localhost:8080{}", path))?;
        let query_pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();

        info!("Query parameters: {:?}", query_pairs);

        // Send response to browser
        let response = if query_pairs.contains_key("code") {
            "HTTP/1.1 200 OK\r\n\r\n<html><body><h1>Authentication Successful!</h1><p>You can close this window.</p></body></html>"
        } else {
            "HTTP/1.1 400 Bad Request\r\n\r\n<html><body><h1>Authentication Failed</h1><p>No authorization code received.</p></body></html>"
        };

        stream.write_all(response.as_bytes()).await?;
        stream.flush().await?;

        // Extract and validate parameters
        let code = query_pairs
            .get("code")
            .ok_or_else(|| anyhow!("No authorization code in callback"))?;

        let state = query_pairs
            .get("state")
            .ok_or_else(|| anyhow!("No state parameter in callback"))?;

        if state.as_ref() != csrf_token.secret() {
            return Err(anyhow!("CSRF token mismatch"));
        }

        // Exchange authorization code for tokens
        info!("Exchanging authorization code for tokens...");
        info!("Client ID: {}", self.config.client_id);
        info!("Redirect URI: {}", self.config.redirect_uri);
        
        // Create a custom HTTP client for public client authentication
        let client = reqwest::Client::new();
        let token_url = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
        
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("code", code),
            ("redirect_uri", &self.config.redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", pkce_verifier.secret()),
        ];
        
        info!("Sending token request with parameters: {:?}", params.iter().map(|(k, _)| k).collect::<Vec<_>>());
        
        let response = client
            .post(token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                error!("HTTP request failed: {:?}", e);
                anyhow!("Failed to send token request: {}", e)
            })?;
        
        let status = response.status();
        let response_text = response.text().await.map_err(|e| {
            error!("Failed to read response: {:?}", e);
            anyhow!("Failed to read response: {}", e)
        })?;
        
        info!("Token response status: {}", status);
        info!("Token response body: {}", response_text);
        
        if !status.is_success() {
            error!("Token exchange failed with status {}: {}", status, response_text);
            return Err(anyhow!("Token exchange failed with status {}: {}", status, response_text));
        }
        
        let token_response: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!("Failed to parse token response JSON: {:?}", e);
                anyhow!("Failed to parse token response: {}", e)
            })?;
        
        let access_token = token_response["access_token"]
            .as_str()
            .ok_or_else(|| anyhow!("No access_token in response"))?;
        
        let refresh_token = token_response["refresh_token"].as_str();
        
        let expires_in = token_response["expires_in"]
            .as_u64()
            .unwrap_or(3600);
        
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + expires_in;

        // Store tokens
        self.tokens = Some(TokenData {
            access_token: access_token.to_string(),
            refresh_token: refresh_token.map(|t| t.to_string()),
            expires_at,
            token_type: "Bearer".to_string(),
        });

        self.save_tokens()?;
        info!("Authentication successful");

        Ok(())
    }

    async fn refresh_access_token(&mut self, refresh_token: String) -> Result<()> {
        info!("Refreshing access token");

        let token_result = self
            .oauth_client
            .exchange_refresh_token(&RefreshToken::new(refresh_token))
            .request_async(async_http_client)
            .await?;

        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + token_result.expires_in().map(|d| d.as_secs()).unwrap_or(3600);

        // Update tokens
        if let Some(ref mut tokens) = self.tokens {
            tokens.access_token = token_result.access_token().secret().clone();
            tokens.expires_at = expires_at;
            
            // Update refresh token if provided
            if let Some(new_refresh_token) = token_result.refresh_token() {
                tokens.refresh_token = Some(new_refresh_token.secret().clone());
            }
        }

        self.save_tokens()?;
        info!("Access token refreshed successfully");

        Ok(())
    }

    pub fn logout(&mut self) -> Result<()> {
        self.tokens = None;
        if self.config.token_file.exists() {
            fs::remove_file(&self.config.token_file)?;
        }
        info!("Logged out successfully");
        Ok(())
    }

    pub fn get_user_email(&self) -> Option<String> {
        // This would typically be extracted from the ID token
        // For now, return None and fetch from API when needed
        None
    }
}
