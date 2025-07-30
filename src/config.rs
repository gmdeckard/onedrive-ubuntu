use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub client_id: String,
    pub redirect_uri: String,
    pub sync_folder: PathBuf,
    pub sync_interval_minutes: u64,
    pub auto_start: bool,
    pub minimize_to_tray: bool,
    pub notifications: bool,
    pub debug_logging: bool,
    
    // Internal paths (not serialized)
    #[serde(skip)]
    pub config_dir: PathBuf,
    #[serde(skip)]
    pub config_file: PathBuf,
    #[serde(skip)]
    pub token_file: PathBuf,
    #[serde(skip)]
    pub db_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config"))
            .join("onedrive-ubuntu");
        
        let sync_folder = dirs::home_dir()
            .unwrap_or_else(|| "/tmp".into())
            .join("OneDrive");

        Self {
            // Default client ID - user will need to configure their own
            client_id: "your-client-id-here".to_string(),
            redirect_uri: "http://localhost:8080".to_string(),
            sync_folder,
            sync_interval_minutes: 5,
            auto_start: true,
            minimize_to_tray: true,
            notifications: true,
            debug_logging: false,
            
            config_file: config_dir.join("config.toml"),
            token_file: config_dir.join("tokens.json"),
            db_file: config_dir.join("sync.db"),
            config_dir,
        }
    }
}

impl Config {
    pub fn new() -> Result<Self> {
        let mut config = Self::default();
        
        // Create config directory
        fs::create_dir_all(&config.config_dir)?;
        info!("Config directory: {}", config.config_dir.display());
        
        // Load existing config if it exists
        if config.config_file.exists() {
            match config.load_from_file() {
                Ok(loaded_config) => {
                    config = loaded_config;
                    info!("Configuration loaded from file");
                }
                Err(e) => {
                    warn!("Failed to load config file, using defaults: {}", e);
                }
            }
        } else {
            // Save default config
            if let Err(e) = config.save() {
                warn!("Failed to save default config: {}", e);
            }
        }
        
        // Ensure sync folder exists
        fs::create_dir_all(&config.sync_folder)?;
        
        Ok(config)
    }
    
    fn load_from_file(&self) -> Result<Self> {
        let content = fs::read_to_string(&self.config_file)?;
        let mut config: Config = toml::from_str(&content)?;
        
        // Set internal paths
        config.config_dir = self.config_dir.clone();
        config.config_file = self.config_file.clone();
        config.token_file = self.token_file.clone();
        config.db_file = self.db_file.clone();
        
        Ok(config)
    }
    
    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write(&self.config_file, content)?;
        info!("Configuration saved");
        Ok(())
    }
    
    pub fn update_sync_folder(&mut self, new_path: PathBuf) -> Result<()> {
        fs::create_dir_all(&new_path)?;
        self.sync_folder = new_path;
        self.save()?;
        Ok(())
    }
    
    pub fn set_auto_start(&mut self, enabled: bool) -> Result<()> {
        self.auto_start = enabled;
        self.save()?;
        Ok(())
    }
    
    pub fn set_minimize_to_tray(&mut self, enabled: bool) -> Result<()> {
        self.minimize_to_tray = enabled;
        self.save()?;
        Ok(())
    }
    
    pub fn set_notifications(&mut self, enabled: bool) -> Result<()> {
        self.notifications = enabled;
        self.save()?;
        Ok(())
    }
    
    pub fn set_debug_logging(&mut self, enabled: bool) -> Result<()> {
        self.debug_logging = enabled;
        self.save()?;
        Ok(())
    }
    
    pub fn set_sync_interval(&mut self, minutes: u64) -> Result<()> {
        self.sync_interval_minutes = minutes;
        self.save()?;
        Ok(())
    }
    
    pub fn update_azure_config(&mut self, client_id: String, redirect_uri: String) -> Result<()> {
        self.client_id = client_id;
        self.redirect_uri = redirect_uri;
        self.save()?;
        Ok(())
    }
}
