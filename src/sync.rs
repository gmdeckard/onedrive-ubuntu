use anyhow::{Result, anyhow};
use rusqlite::{Connection, params};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{interval, Duration};
use tracing::{info, error, debug, warn};
use walkdir::WalkDir;

use crate::api::{OneDriveAPI, DriveItem};
use crate::config::Config;

#[derive(Debug, Clone)]
pub enum SyncAction {
    Upload { local_path: String, remote_path: String },
    Download { remote_item: DriveItem, local_path: String },
    RemoveFromDatabase { path: String },
}

#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub last_sync: Option<SystemTime>,
    pub files_uploaded: u64,
    pub files_downloaded: u64,
    pub files_deleted: u64,
    pub sync_errors: Vec<String>,
    pub total_files: u64,
    pub current_operation: String,
    pub sync_progress: f32, // 0.0 to 1.0
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            is_syncing: false,
            last_sync: None,
            files_uploaded: 0,
            files_downloaded: 0,
            files_deleted: 0,
            sync_errors: Vec::new(),
            total_files: 0,
            current_operation: "Ready".to_string(),
            sync_progress: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub modified: u64,
    pub onedrive_id: Option<String>,
    pub last_synced: u64,
}

#[derive(Debug, Clone)]
pub struct SyncLogEntry {
    pub timestamp: u64,
    pub action: String,
    pub file_path: String,
    pub status: String,
    pub error: Option<String>,
}

pub struct SyncManager {
    config: Arc<Config>,
    api: Arc<OneDriveAPI>,
    db: Arc<TokioMutex<Connection>>,
    status: Arc<TokioMutex<SyncStatus>>,
}

impl SyncManager {
    pub fn new(config: Arc<Config>, api: Arc<OneDriveAPI>) -> Result<Self> {
        let db = Connection::open(&config.db_file)?;
        
        // Initialize database schema
        db.execute(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                modified INTEGER NOT NULL,
                onedrive_id TEXT,
                last_synced INTEGER NOT NULL
            )",
            [],
        )?;

        db.execute(
            "CREATE TABLE IF NOT EXISTS sync_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                action TEXT NOT NULL,
                file_path TEXT NOT NULL,
                status TEXT NOT NULL,
                error TEXT
            )",
            [],
        )?;

        info!("Sync database initialized");

        Ok(Self {
            config,
            api,
            db: Arc::new(TokioMutex::new(db)),
            status: Arc::new(TokioMutex::new(SyncStatus::default())),
        })
    }

    pub async fn get_status(&self) -> SyncStatus {
        self.status.lock().await.clone()
    }

    pub async fn update_status<F>(&self, updater: F)
    where
        F: FnOnce(&mut SyncStatus),
    {
        let mut status = self.status.lock().await;
        updater(&mut *status);
    }

    pub async fn start_auto_sync(&mut self) {
        let sync_interval_secs = self.config.sync_interval_minutes * 60;
        let mut interval = interval(Duration::from_secs(sync_interval_secs));
        
        info!("Starting auto-sync every {} minutes", self.config.sync_interval_minutes);

        loop {
            interval.tick().await;
            
            let is_syncing = {
                let status = self.status.lock().await;
                status.is_syncing
            };
            
            if !is_syncing {
                info!("Starting automatic sync");
                if let Err(e) = self.sync().await {
                    error!("Auto-sync failed: {}", e);
                    self.update_status(|status| {
                        status.sync_errors.push(format!("Auto-sync failed: {}", e));
                    }).await;
                }
            } else {
                debug!("Skipping auto-sync - sync already in progress");
            }
        }
    }

    pub async fn sync(&mut self) -> Result<()> {
        let is_syncing = {
            let status = self.status.lock().await;
            status.is_syncing
        };
        
        if is_syncing {
            return Err(anyhow!("Sync already in progress"));
        }

        self.update_status(|status| {
            status.is_syncing = true;
            status.sync_errors.clear();
            status.current_operation = "Starting sync...".to_string();
            status.sync_progress = 0.0;
        }).await;
        
        info!("Starting bidirectional sync");
        
        let sync_result = self.perform_sync().await;
        
        self.update_status(|status| {
            status.is_syncing = false;
            status.last_sync = Some(SystemTime::now());
            status.sync_progress = 1.0;
        }).await;
        
        match sync_result {
            Ok(_) => {
                info!("Sync completed successfully");
                self.update_status(|status| {
                    status.current_operation = "Sync completed".to_string();
                }).await;
                self.log_sync_event("sync_complete", "", "success", None).await?;
            }
            Err(e) => {
                error!("Sync failed: {}", e);
                self.update_status(|status| {
                    status.sync_errors.push(e.to_string());
                    status.current_operation = "Sync failed".to_string();
                }).await;
                self.log_sync_event("sync_complete", "", "failed", Some(&e.to_string())).await?;
                return Err(e);
            }
        }

        Ok(())
    }

    async fn perform_sync(&mut self) -> Result<()> {
        info!("=== STARTING SYNC PROCESS ===");
        
        // Step 1: Get local file state
        self.update_status(|status| {
            status.current_operation = "Scanning local files...".to_string();
            status.sync_progress = 0.1;
        }).await;
        
        let local_files = self.scan_local_files().await?;
        info!("=== LOCAL SCAN COMPLETE: {} files ===", local_files.len());

        // Step 2: Get remote file state
        self.update_status(|status| {
            status.current_operation = "Scanning remote files...".to_string();
            status.sync_progress = 0.3;
        }).await;
        
        let remote_files = self.scan_remote_files().await?;
        info!("=== REMOTE SCAN COMPLETE: {} files ===", remote_files.len());

        // Step 3: Get stored sync state
        self.update_status(|status| {
            status.current_operation = "Loading sync database...".to_string();
            status.sync_progress = 0.4;
        }).await;
        
        let stored_files = self.get_stored_files()?;
        info!("=== DATABASE SCAN COMPLETE: {} files ===", stored_files.len());

        // Step 4: Determine sync actions
        self.update_status(|status| {
            status.current_operation = "Determining sync actions...".to_string();
            status.sync_progress = 0.5;
        }).await;
        
        let actions = self.determine_sync_actions(&local_files, &remote_files, &stored_files)?;
        info!("=== SYNC ACTIONS DETERMINED: {} actions ===", actions.len());

        // Update total files count
        self.update_status(|status| {
            status.total_files = (local_files.len() + remote_files.len()) as u64;
        }).await;

        // Step 5: Execute sync actions
        let total_actions = actions.len();
        if total_actions == 0 {
            info!("=== NO SYNC ACTIONS NEEDED - EVERYTHING UP TO DATE ===");
            self.update_status(|status| {
                status.current_operation = "All files are up to date".to_string();
                status.sync_progress = 1.0;
            }).await;
        } else {
            info!("=== EXECUTING {} SYNC ACTIONS ===", total_actions);
            for (i, action) in actions.into_iter().enumerate() {
                let progress = 0.5 + (0.4 * (i as f32 / total_actions as f32));
                
                let operation_desc = match &action {
                    SyncAction::Upload { local_path, .. } => format!("Uploading {}", local_path),
                    SyncAction::Download { local_path, .. } => format!("Downloading {}", local_path),
                    SyncAction::RemoveFromDatabase { path } => format!("Cleaning up {}", path),
                };
                
                info!("=== EXECUTING: {} ===", operation_desc);
                
                self.update_status(|status| {
                    status.current_operation = operation_desc;
                    status.sync_progress = progress;
                }).await;
                
                if let Err(e) = self.execute_sync_action(action).await {
                    error!("Sync action failed: {}", e);
                    self.update_status(|status| {
                        status.sync_errors.push(e.to_string());
                    }).await;
                    // Continue with other actions
                }
            }
        }

        info!("=== SYNC PROCESS COMPLETE ===");
        Ok(())
    }

    async fn scan_local_files(&self) -> Result<HashMap<String, FileRecord>> {
        let mut files = HashMap::new();
        
        if !self.config.sync_folder.exists() {
            info!("Creating sync folder: {}", self.config.sync_folder.display());
            fs::create_dir_all(&self.config.sync_folder).await?;
            return Ok(files);
        }

        info!("Scanning local files in: {}", self.config.sync_folder.display());
        
        for entry in WalkDir::new(&self.config.sync_folder)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path();
                let relative_path = path.strip_prefix(&self.config.sync_folder)?;
                let relative_path_str = relative_path.to_string_lossy().replace('\\', "/");

                // Skip hidden files and system files
                if relative_path_str.starts_with('.') {
                    continue;
                }

                if let Ok(metadata) = entry.metadata() {
                    let size = metadata.len();
                    let modified = metadata
                        .modified()
                        .unwrap_or(SystemTime::UNIX_EPOCH)
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    let hash = self.calculate_file_hash(path).await.unwrap_or_else(|e| {
                        warn!("Failed to calculate hash for {}: {}", path.display(), e);
                        String::new()
                    });

                    info!("Found local file: {} (size: {}, hash: {})", relative_path_str, size, &hash[..8]);

                    files.insert(relative_path_str.clone(), FileRecord {
                        path: relative_path_str,
                        hash,
                        size,
                        modified,
                        onedrive_id: None,
                        last_synced: 0,
                    });
                }
            }
        }

        info!("Scanned {} local files", files.len());
        Ok(files)
    }

    async fn scan_remote_files(&self) -> Result<HashMap<String, DriveItem>> {
        let mut files = HashMap::new();
        
        info!("Scanning remote OneDrive files...");
        
        match self.scan_remote_folder(&mut files, "/").await {
            Ok(_) => {
                info!("Scanned {} remote files", files.len());
                Ok(files)
            }
            Err(e) => {
                error!("Failed to scan remote files: {}", e);
                // Return empty map instead of failing completely
                Ok(HashMap::new())
            }
        }
    }

    fn scan_remote_folder<'a>(&'a self, files: &'a mut HashMap<String, DriveItem>, folder_path: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let items = self.api.list_items(folder_path).await?;
            
            for item in items {
                let item_path = if folder_path == "/" {
                    item.name.clone()
                } else {
                    format!("{}/{}", folder_path.trim_start_matches('/'), item.name)
                };

                if item.file.is_some() {
                    files.insert(item_path, item);
                } else if item.folder.is_some() {
                    // Recursively scan subfolders
                    self.scan_remote_folder(files, &format!("/{}", item_path)).await?;
                }
            }

            Ok(())
        })
    }

    fn get_stored_files(&self) -> Result<HashMap<String, FileRecord>> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let db = self.db.lock().await;
            let mut files = HashMap::new();
            
            let mut stmt = db.prepare(
                "SELECT path, hash, size, modified, onedrive_id, last_synced FROM files"
            )?;

            let file_iter = stmt.query_map([], |row| {
                Ok(FileRecord {
                    path: row.get(0)?,
                    hash: row.get(1)?,
                    size: row.get(2)?,
                    modified: row.get(3)?,
                    onedrive_id: row.get(4)?,
                    last_synced: row.get(5)?,
                })
            })?;

            for file in file_iter {
                let file = file?;
                files.insert(file.path.clone(), file);
            }

            Ok(files)
        })
    }

    fn determine_sync_actions(
        &self,
        local_files: &HashMap<String, FileRecord>,
        remote_files: &HashMap<String, DriveItem>,
        stored_files: &HashMap<String, FileRecord>,
    ) -> Result<Vec<SyncAction>> {
        let mut actions = Vec::new();

        info!("Determining sync actions...");
        info!("Local files: {}, Remote files: {}, Stored files: {}", 
              local_files.len(), remote_files.len(), stored_files.len());

        // Check for uploads (local files not in remote or modified locally)
        for (path, local_file) in local_files {
            info!("Checking local file: {}", path);
            
            if let Some(stored_file) = stored_files.get(path) {
                if local_file.hash != stored_file.hash {
                    // File modified locally
                    info!("Local file modified: {} (hash changed)", path);
                    actions.push(SyncAction::Upload {
                        local_path: path.clone(),
                        remote_path: path.clone(),
                    });
                } else {
                    info!("Local file unchanged: {}", path);
                }
            } else if !remote_files.contains_key(path) {
                // New local file
                info!("New local file found: {}", path);
                actions.push(SyncAction::Upload {
                    local_path: path.clone(),
                    remote_path: path.clone(),
                });
            } else {
                info!("Local file exists remotely but not in database: {}", path);
                // File exists remotely but not in our database - treat as already synced
                // This can happen if database was cleared
            }
        }

        // Check for downloads (remote files not in local or modified remotely)
        for (path, remote_file) in remote_files {
            info!("Checking remote file: {}", path);
            
            if !local_files.contains_key(path) {
                // New remote file
                info!("New remote file found: {}", path);
                actions.push(SyncAction::Download {
                    remote_item: remote_file.clone(),
                    local_path: path.clone(),
                });
            } else if let Some(stored_file) = stored_files.get(path) {
                // Check if remote file is newer (simplified comparison)
                let remote_modified = parse_iso_datetime(&remote_file.last_modified).unwrap_or(0);
                if remote_modified > stored_file.last_synced {
                    info!("Remote file newer than local: {}", path);
                    actions.push(SyncAction::Download {
                        remote_item: remote_file.clone(),
                        local_path: path.clone(),
                    });
                } else {
                    info!("Remote file up to date: {}", path);
                }
            } else {
                info!("Remote file exists locally but not in database: {}", path);
            }
        }

        // Check for deletions (files in stored but not in local or remote)
        for (path, _) in stored_files {
            if !local_files.contains_key(path) && !remote_files.contains_key(path) {
                info!("File deleted both locally and remotely: {}", path);
                actions.push(SyncAction::RemoveFromDatabase {
                    path: path.clone(),
                });
            }
        }

        info!("Determined {} sync actions", actions.len());
        for action in &actions {
            match action {
                SyncAction::Upload { local_path, .. } => info!("Action: Upload {}", local_path),
                SyncAction::Download { local_path, .. } => info!("Action: Download {}", local_path),
                SyncAction::RemoveFromDatabase { path } => info!("Action: Cleanup {}", path),
            }
        }

        Ok(actions)
    }

    async fn execute_sync_action(&mut self, action: SyncAction) -> Result<()> {
        match action {
            SyncAction::Upload { local_path, remote_path } => {
                let local_full_path = self.config.sync_folder.join(&local_path);
                
                info!("Uploading: {}", local_path);
                let remote_item = self.api.upload_file(&local_full_path, &remote_path).await?;
                
                // Update database
                let hash = self.calculate_file_hash(&local_full_path).await?;
                let metadata = fs::metadata(&local_full_path).await?;
                let size = metadata.len();
                let modified = metadata
                    .modified()?
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs();
                let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();

                let db = self.db.lock().await;
                db.execute(
                    "INSERT OR REPLACE INTO files (path, hash, size, modified, onedrive_id, last_synced) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![local_path, hash, size, modified, remote_item.id, now],
                )?;
                drop(db);

                self.update_status(|status| {
                    status.files_uploaded += 1;
                }).await;
                self.log_sync_event("upload", &local_path, "success", None).await?;
            }

            SyncAction::Download { remote_item, local_path } => {
                let local_full_path = self.config.sync_folder.join(&local_path);
                
                // Create parent directories if needed
                if let Some(parent) = local_full_path.parent() {
                    fs::create_dir_all(parent).await?;
                }
                
                info!("Downloading: {}", local_path);
                self.api.download_file(&remote_item, &local_full_path).await?;
                
                // Update database
                let hash = self.calculate_file_hash(&local_full_path).await?;
                let size = remote_item.size.unwrap_or(0);
                let modified = parse_iso_datetime(&remote_item.last_modified).unwrap_or(0);
                let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();

                let db = self.db.lock().await;
                db.execute(
                    "INSERT OR REPLACE INTO files (path, hash, size, modified, onedrive_id, last_synced) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![local_path, hash, size, modified, remote_item.id, now],
                )?;
                drop(db);

                self.update_status(|status| {
                    status.files_downloaded += 1;
                }).await;
                self.log_sync_event("download", &local_path, "success", None).await?;
            }

            SyncAction::RemoveFromDatabase { path } => {
                let db = self.db.lock().await;
                db.execute("DELETE FROM files WHERE path = ?1", params![path])?;
                drop(db);
                
                self.update_status(|status| {
                    status.files_deleted += 1;
                }).await;
                self.log_sync_event("remove_from_db", &path, "success", None).await?;
            }
        }

        Ok(())
    }

    async fn calculate_file_hash(&self, path: &Path) -> Result<String> {
        let content = fs::read(path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(hex::encode(hasher.finalize()))
    }

    async fn log_sync_event(&self, action: &str, file_path: &str, status: &str, error: Option<&str>) -> Result<()> {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
        
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO sync_log (timestamp, action, file_path, status, error) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now, action, file_path, status, error],
        )?;
        drop(db);

        Ok(())
    }

    pub async fn get_sync_history(&self, limit: usize) -> Result<Vec<SyncLogEntry>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT timestamp, action, file_path, status, error FROM sync_log ORDER BY timestamp DESC LIMIT ?1"
        )?;

        let entries = stmt.query_map(params![limit], |row| {
            Ok(SyncLogEntry {
                timestamp: row.get(0)?,
                action: row.get(1)?,
                file_path: row.get(2)?,
                status: row.get(3)?,
                error: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }

        Ok(result)
    }
}

fn parse_iso_datetime(_datetime_str: &str) -> Option<u64> {
    // Simplified ISO datetime parsing
    // In a real implementation, use a proper datetime parsing library
    // For now, return current timestamp as placeholder
    Some(
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    )
}
