use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{info, error};

use crate::auth::AuthManager;

#[derive(Debug, Clone, Deserialize)]
pub struct DriveItem {
    pub id: String,
    pub name: String,
    #[serde(rename = "lastModifiedDateTime")]
    pub last_modified: String,
    pub size: Option<u64>,
    pub file: Option<serde_json::Value>,
    pub folder: Option<serde_json::Value>,
    #[serde(rename = "@microsoft.graph.downloadUrl")]
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriveResponse {
    pub value: Vec<DriveItem>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub mail: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriveInfo {
    pub id: String,
    #[serde(rename = "driveType")]
    pub drive_type: String,
    pub quota: Option<DriveQuota>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriveQuota {
    pub total: u64,
    pub used: u64,
    pub remaining: u64,
}

pub struct OneDriveAPI {
    client: Client,
    auth: Arc<Mutex<AuthManager>>,
    base_url: String,
}

impl OneDriveAPI {
    pub fn new(auth: Arc<Mutex<AuthManager>>) -> Self {
        Self {
            client: Client::new(),
            auth,
            base_url: "https://graph.microsoft.com/v1.0".to_string(),
        }
    }

    async fn get_auth_header(&self) -> Result<String> {
        let mut auth = self.auth.lock().await;
        let token = auth.get_access_token().await?;
        Ok(format!("Bearer {}", token))
    }

    pub async fn get_user_info(&self) -> Result<UserInfo> {
        let auth_header = self.get_auth_header().await?;
        
        let response = self
            .client
            .get(&format!("{}/me", self.base_url))
            .header("Authorization", auth_header)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Failed to get user info: {}", error_text);
            return Err(anyhow!("Failed to get user info: {}", error_text));
        }

        let user_info: UserInfo = response.json().await?;
        info!("Retrieved user info for: {}", user_info.display_name);
        Ok(user_info)
    }

    pub async fn get_drive_info(&self) -> Result<DriveInfo> {
        let auth_header = self.get_auth_header().await?;
        
        let response = self
            .client
            .get(&format!("{}/me/drive", self.base_url))
            .header("Authorization", auth_header)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Failed to get drive info: {}", error_text);
            return Err(anyhow!("Failed to get drive info: {}", error_text));
        }

        let drive_info: DriveInfo = response.json().await?;
        info!("Retrieved drive info: {} ({})", drive_info.id, drive_info.drive_type);
        Ok(drive_info)
    }

    pub async fn list_root_items(&self) -> Result<Vec<DriveItem>> {
        self.list_items("/").await
    }

    pub async fn list_items(&self, path: &str) -> Result<Vec<DriveItem>> {
        let auth_header = self.get_auth_header().await?;
        
        let url = if path == "/" {
            format!("{}/me/drive/root/children", self.base_url)
        } else {
            format!("{}/me/drive/root:{}:/children", self.base_url, path)
        };

        let mut all_items = Vec::new();
        let mut next_url = Some(url);

        while let Some(url) = next_url {
            let response = self
                .client
                .get(&url)
                .header("Authorization", auth_header.clone())
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                error!("Failed to list items: {}", error_text);
                return Err(anyhow!("Failed to list items: {}", error_text));
            }

            let drive_response: DriveResponse = response.json().await?;
            all_items.extend(drive_response.value);
            next_url = drive_response.next_link;
        }

        info!("Listed {} items from path: {}", all_items.len(), path);
        Ok(all_items)
    }

    pub async fn download_file(&self, item: &DriveItem, local_path: &Path) -> Result<()> {
        let download_url = if let Some(url) = &item.download_url {
            url.clone()
        } else {
            // Get download URL from item ID
            let auth_header = self.get_auth_header().await?;
            let response = self
                .client
                .get(&format!("{}/me/drive/items/{}/content", self.base_url, item.id))
                .header("Authorization", auth_header)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                error!("Failed to get download URL: {}", error_text);
                return Err(anyhow!("Failed to get download URL: {}", error_text));
            }

            response.url().to_string()
        };

        // Download the file
        let response = self.client.get(&download_url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to download file: HTTP {}", response.status()));
        }

        // Create parent directories
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write file content
        let content = response.bytes().await?;
        let mut file = fs::File::create(local_path).await?;
        file.write_all(&content).await?;

        info!("Downloaded file: {} -> {}", item.name, local_path.display());
        Ok(())
    }

    pub async fn upload_file(&self, local_path: &Path, remote_name: &str) -> Result<DriveItem> {
        let auth_header = self.get_auth_header().await?;
        
        // Read file content
        let content = fs::read(local_path).await?;
        let file_size = content.len();

        info!("Uploading file: {} ({} bytes)", remote_name, file_size);

        // For files smaller than 4MB, use simple upload
        if file_size < 4 * 1024 * 1024 {
            let url = format!("{}/me/drive/root:/{remote_name}:/content", self.base_url);
            
            let response = self
                .client
                .put(&url)
                .header("Authorization", auth_header)
                .header("Content-Type", "application/octet-stream")
                .body(content)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                error!("Failed to upload file: {}", error_text);
                return Err(anyhow!("Failed to upload file: {}", error_text));
            }

            let item: DriveItem = response.json().await?;
            info!("Successfully uploaded file: {}", remote_name);
            Ok(item)
        } else {
            // Use resumable upload for larger files
            self.upload_large_file(local_path, remote_name, content).await
        }
    }

    async fn upload_large_file(&self, _local_path: &Path, remote_name: &str, content: Vec<u8>) -> Result<DriveItem> {
        let auth_header = self.get_auth_header().await?;
        
        // Create upload session
        let session_url = format!("{}/me/drive/root:/{remote_name}:/createUploadSession", self.base_url);
        let session_body = serde_json::json!({
            "item": {
                "@microsoft.graph.conflictBehavior": "replace"
            }
        });

        let response = self
            .client
            .post(&session_url)
            .header("Authorization", auth_header.clone())
            .header("Content-Type", "application/json")
            .json(&session_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Failed to create upload session: {}", error_text));
        }

        #[derive(Deserialize)]
        struct UploadSession {
            #[serde(rename = "uploadUrl")]
            upload_url: String,
        }

        let session: UploadSession = response.json().await?;
        
        // Upload file in chunks
        let chunk_size = 320 * 1024; // 320KB chunks
        let total_size = content.len();
        let mut offset = 0;

        while offset < total_size {
            let end = std::cmp::min(offset + chunk_size, total_size);
            let chunk = &content[offset..end];
            
            let content_range = format!("bytes {}-{}/{}", offset, end - 1, total_size);
            
            let response = self
                .client
                .put(&session.upload_url)
                .header("Content-Range", content_range)
                .header("Content-Length", chunk.len().to_string())
                .body(chunk.to_vec())
                .send()
                .await?;

            if response.status().as_u16() == 202 {
                // Chunk uploaded successfully, continue
                offset = end;
                info!("Uploaded chunk: {}/{} bytes", end, total_size);
            } else if response.status().as_u16() == 201 || response.status().as_u16() == 200 {
                // Upload complete
                let item: DriveItem = response.json().await?;
                info!("Successfully uploaded large file: {}", remote_name);
                return Ok(item);
            } else {
                let error_text = response.text().await?;
                return Err(anyhow!("Upload chunk failed: {}", error_text));
            }
        }

        Err(anyhow!("Upload completed but no final response received"))
    }

    pub async fn delete_item(&self, item_id: &str) -> Result<()> {
        let auth_header = self.get_auth_header().await?;
        
        let response = self
            .client
            .delete(&format!("{}/me/drive/items/{}", self.base_url, item_id))
            .header("Authorization", auth_header)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Failed to delete item: {}", error_text);
            return Err(anyhow!("Failed to delete item: {}", error_text));
        }

        info!("Successfully deleted item: {}", item_id);
        Ok(())
    }

    pub async fn create_folder(&self, folder_name: &str, parent_path: &str) -> Result<DriveItem> {
        let auth_header = self.get_auth_header().await?;
        
        let url = if parent_path == "/" {
            format!("{}/me/drive/root/children", self.base_url)
        } else {
            format!("{}/me/drive/root:{}:/children", self.base_url, parent_path)
        };

        let folder_data = serde_json::json!({
            "name": folder_name,
            "folder": {}
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .json(&folder_data)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Failed to create folder: {}", error_text);
            return Err(anyhow!("Failed to create folder: {}", error_text));
        }

        let item: DriveItem = response.json().await?;
        info!("Successfully created folder: {}", folder_name);
        Ok(item)
    }
}
