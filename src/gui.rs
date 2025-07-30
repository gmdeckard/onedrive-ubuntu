use eframe::egui;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, error};

use crate::api::{OneDriveAPI, UserInfo, DriveInfo};
use crate::auth::AuthManager;
use crate::config::Config;
use crate::sync::{SyncManager, SyncStatus, SyncLogEntry};

pub struct OneDriveApp {
    config: Arc<Config>,
    auth: Arc<Mutex<AuthManager>>,
    sync_manager: Arc<Mutex<SyncManager>>,
    
    // UI state
    current_tab: Tab,
    user_info: Option<UserInfo>,
    drive_info: Option<DriveInfo>,
    sync_status: SyncStatus,
    status_message: String,
    
    // Logs cache
    sync_history_cache: Vec<SyncLogEntry>,
    last_history_refresh: std::time::Instant,
    
    // Settings state
    new_sync_folder: String,
    
    // Setup wizard state
    show_setup_wizard: bool,
    setup_step: SetupStep,
    client_id_input: String,
    
    // Runtime
    rt: tokio::runtime::Runtime,
}

#[derive(Debug, Clone, PartialEq)]
enum Tab {
    Status,
    Settings,
    Logs,
}

#[derive(Debug, Clone, PartialEq)]
enum SetupStep {
    Welcome,
    AzureInstructions,
    ClientIdInput,
    Complete,
}

impl OneDriveApp {
    pub fn new(
        config: Arc<Config>,
        auth: Arc<Mutex<AuthManager>>,
        sync_manager: Arc<Mutex<SyncManager>>,
    ) -> Self {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        
        // Check if we need to show setup wizard (if using default client ID)
        let needs_setup = config.client_id == "14d82eec-204b-4c2f-b7e8-296a70dab67e";
        
        let mut app = Self {
            config: config.clone(),
            auth,
            sync_manager,
            current_tab: Tab::Status,
            user_info: None,
            drive_info: None,
            sync_status: SyncStatus::default(),
            status_message: "Welcome to OneDrive Ubuntu Client".to_string(),
            sync_history_cache: Vec::new(),
            last_history_refresh: std::time::Instant::now(),
            new_sync_folder: config.sync_folder.to_string_lossy().to_string(),
            show_setup_wizard: needs_setup,
            setup_step: SetupStep::Welcome,
            client_id_input: String::new(),
            rt,
        };
        
        // Load initial data
        if !needs_setup {
            app.refresh_data();
        }
        
        app
    }
    
    fn refresh_data(&mut self) {
        let auth = self.auth.clone();
        let api = Arc::new(OneDriveAPI::new(auth.clone()));
        
        // Check authentication status
        let is_authenticated = self.rt.block_on(async {
            let auth_guard = auth.lock().await;
            auth_guard.is_authenticated()
        });
        
        if is_authenticated {
            // Load user info
            let api_clone = api.clone();
            if let Ok(user_info) = self.rt.block_on(async {
                api_clone.get_user_info().await
            }) {
                self.user_info = Some(user_info);
            }
            
            // Load drive info
            let api_clone = api.clone();
            if let Ok(drive_info) = self.rt.block_on(async {
                api_clone.get_drive_info().await
            }) {
                self.drive_info = Some(drive_info);
            }
            
            self.status_message = "✓ Authenticated and ready to sync".to_string();
            
            // Trigger initial sync if this is the first time we're authenticated
            if self.user_info.is_some() {
                let sync_manager = self.sync_manager.clone();
                let _ = self.rt.spawn(async move {
                    // Wait a moment for everything to initialize
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    
                    let mut sync_guard = sync_manager.lock().await;
                    info!("Triggering initial sync after authentication");
                    match sync_guard.sync().await {
                        Ok(_) => info!("Initial sync completed"),
                        Err(e) => error!("Initial sync failed: {}", e),
                    }
                });
            }
        } else {
            self.status_message = "⚠ Please authenticate with Microsoft to enable sync".to_string();
        }
        
        // Update sync status
        self.sync_status = self.rt.block_on(async {
            let sync_guard = self.sync_manager.lock().await;
            sync_guard.get_status().await
        });
    }
}

impl eframe::App for OneDriveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check authentication status periodically 
        let is_authenticated = self.rt.block_on(async {
            let auth_guard = self.auth.lock().await;
            auth_guard.is_authenticated()
        });
        
        // Update user info and status if authentication state changed
        if is_authenticated && self.user_info.is_none() {
            // Authentication completed, refresh user data
            self.refresh_data();
            self.status_message = "Authentication successful".to_string();
        } else if !is_authenticated && self.user_info.is_some() {
            // Authentication lost, clear user data
            self.user_info = None;
            self.drive_info = None;
            self.status_message = "⚠ Please authenticate with Microsoft to enable sync".to_string();
        }
        
        // Show setup wizard if needed
        if self.show_setup_wizard {
            self.show_setup_wizard_ui(ctx);
            return;
        }
        
        // Update sync status periodically
        self.sync_status = {
            self.rt.block_on(async {
                if let Ok(sync_guard) = tokio::time::timeout(
                    std::time::Duration::from_millis(10),
                    self.sync_manager.lock()
                ).await {
                    sync_guard.get_status().await
                } else {
                    self.sync_status.clone()
                }
            })
        };
        
        // Top menu bar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        // Show about dialog
                    }
                });
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Status indicator
                    let (icon, color) = if self.user_info.is_some() {
                        if self.sync_status.is_syncing {
                            ("🔄", egui::Color32::BLUE)
                        } else {
                            ("✓", egui::Color32::GREEN)
                        }
                    } else {
                        ("⚠", egui::Color32::YELLOW)
                    };
                    
                    ui.colored_label(color, icon);
                    ui.label(&self.status_message);
                });
            });
        });
        
        // Tab buttons
        egui::TopBottomPanel::top("tab_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, Tab::Status, "Status");
                ui.selectable_value(&mut self.current_tab, Tab::Settings, "Settings");
                ui.selectable_value(&mut self.current_tab, Tab::Logs, "Logs");
            });
        });
        
        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_tab {
                Tab::Status => self.show_status_tab(ui, ctx),
                Tab::Settings => self.show_settings_tab(ui),
                Tab::Logs => self.show_logs_tab(ui),
            }
        });
        
        // Request repaint for real-time updates
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
    }
}

impl OneDriveApp {
    fn show_status_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("OneDrive Status");
        
        ui.separator();
        
        // Authentication section
        ui.group(|ui| {
            ui.label("Authentication");
            
            if let Some(ref user_info) = self.user_info {
                ui.label(format!("Signed in as: {}", user_info.display_name));
                ui.label(format!("Email: {}", 
                    user_info.mail.as_ref().unwrap_or(&user_info.user_principal_name)
                ));
                
                ui.horizontal(|ui| {
                    if ui.button("Sign Out").clicked() {
                        self.sign_out();
                    }
                    
                    if ui.button("Refresh").clicked() {
                        self.refresh_data();
                    }
                });
            } else {
                ui.label("Not authenticated");
                
                if ui.button("Sign In with Microsoft").clicked() {
                    self.authenticate(ctx);
                }
            }
        });
        
        ui.add_space(10.0);
        
        // Drive information
        if let Some(ref drive_info) = self.drive_info {
            ui.group(|ui| {
                ui.label("OneDrive Information");
                ui.label(format!("Drive Type: {}", drive_info.drive_type));
                
                if let Some(ref quota) = drive_info.quota {
                    let used_gb = quota.used as f64 / (1024.0 * 1024.0 * 1024.0);
                    let total_gb = quota.total as f64 / (1024.0 * 1024.0 * 1024.0);
                    let used_percent = (quota.used as f64 / quota.total as f64) * 100.0;
                    
                    ui.label(format!("Storage: {:.2} GB / {:.2} GB ({:.1}% used)", 
                        used_gb, total_gb, used_percent));
                    
                    // Progress bar
                    let progress = quota.used as f32 / quota.total as f32;
                    ui.add(egui::ProgressBar::new(progress).text(format!("{:.1}%", used_percent)));
                }
            });
            
            ui.add_space(10.0);
        }
        
        // Sync status section
        ui.group(|ui| {
            ui.label("Synchronization");
            
            ui.label(format!("Sync Folder: {}", self.config.sync_folder.display()));
            
            if self.sync_status.is_syncing {
                ui.label("🔄 Sync in progress...");
                ui.label(&self.sync_status.current_operation);
                
                // Show progress bar
                let progress = self.sync_status.sync_progress;
                ui.add(egui::ProgressBar::new(progress).text(format!("{:.1}%", progress * 100.0)));
                
            } else if let Some(last_sync) = self.sync_status.last_sync {
                let elapsed = std::time::SystemTime::now()
                    .duration_since(last_sync)
                    .unwrap_or_default();
                
                if elapsed.as_secs() < 60 {
                    ui.label("✓ Synced just now");
                } else if elapsed.as_secs() < 3600 {
                    ui.label(format!("✓ Synced {} minutes ago", elapsed.as_secs() / 60));
                } else {
                    ui.label(format!("✓ Synced {} hours ago", elapsed.as_secs() / 3600));
                }
            } else {
                ui.label("⏳ Not synced yet");
            }
            
            // Show current operation even when not syncing for better feedback
            if !self.sync_status.current_operation.is_empty() && self.sync_status.current_operation != "Ready" {
                ui.label(format!("Status: {}", self.sync_status.current_operation));
            }
            
            ui.horizontal(|ui| {
                if ui.button("Sync Now").clicked() && self.user_info.is_some() && !self.sync_status.is_syncing {
                    self.start_manual_sync();
                }
            });
            
            // Show total files and sync statistics
            if self.sync_status.total_files > 0 {
                ui.separator();
                ui.label(format!("Total files tracked: {}", self.sync_status.total_files));
            }
            
            if self.sync_status.files_uploaded > 0 || self.sync_status.files_downloaded > 0 || self.sync_status.files_deleted > 0 {
                ui.separator();
                ui.label("Last Sync Statistics:");
                ui.label(format!("↑ Uploaded: {}", self.sync_status.files_uploaded));
                ui.label(format!("↓ Downloaded: {}", self.sync_status.files_downloaded));
                ui.label(format!("🗑 Deleted: {}", self.sync_status.files_deleted));
            }
            
            // Show errors if any
            if !self.sync_status.sync_errors.is_empty() {
                ui.separator();
                ui.colored_label(egui::Color32::RED, "Recent Errors:");
                for error in &self.sync_status.sync_errors {
                    ui.colored_label(egui::Color32::RED, format!("• {}", error));
                }
            }
        });
    }
    
    fn show_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        
        ui.separator();
        
        // Sync folder settings
        ui.group(|ui| {
            ui.label("Sync Folder");
            
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_sync_folder);
                
                if ui.button("Browse").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.new_sync_folder = path.to_string_lossy().to_string();
                    }
                }
                
                if ui.button("Apply").clicked() {
                    self.update_sync_folder();
                }
            });
        });
        
        ui.add_space(10.0);
        
        // Application settings
        ui.group(|ui| {
            ui.label("Application Settings");
            
            let mut auto_start = self.config.auto_start;
            if ui.checkbox(&mut auto_start, "Start automatically when I sign in").clicked() {
                let mut config = (*self.config).clone();
                if config.set_auto_start(auto_start).is_ok() {
                    // Config updated
                }
            }
            
            let mut minimize_to_tray = self.config.minimize_to_tray;
            if ui.checkbox(&mut minimize_to_tray, "Minimize to system tray").clicked() {
                let mut config = (*self.config).clone();
                if config.set_minimize_to_tray(minimize_to_tray).is_ok() {
                    // Config updated
                }
            }
            
            let mut notifications = self.config.notifications;
            if ui.checkbox(&mut notifications, "Show sync notifications").clicked() {
                let mut config = (*self.config).clone();
                if config.set_notifications(notifications).is_ok() {
                    // Config updated
                }
            }
            
            let mut debug_logging = self.config.debug_logging;
            if ui.checkbox(&mut debug_logging, "Enable debug logging").clicked() {
                let mut config = (*self.config).clone();
                if config.set_debug_logging(debug_logging).is_ok() {
                    // Config updated
                }
            }
        });
        
        ui.add_space(10.0);
        
        // Azure Configuration section
        ui.group(|ui| {
            ui.label("Azure Configuration");
            
            ui.horizontal(|ui| {
                ui.label(format!("Client ID: {}", 
                    if self.config.client_id == "14d82eec-204b-4c2f-b7e8-296a70dab67e" {
                        "Not configured (using default)".to_string()
                    } else {
                        self.config.client_id.clone()
                    }
                ));
            });
            
            ui.horizontal(|ui| {
                if ui.button("🔧 Setup Azure App Registration").clicked() {
                    self.show_setup_wizard = true;
                    self.setup_step = SetupStep::Welcome;
                    self.client_id_input.clear();
                }
                
                if ui.button("📋 Copy Redirect URI").clicked() {
                    ui.output_mut(|o| o.copied_text = "http://localhost:8080/callback".to_string());
                    self.status_message = "Redirect URI copied to clipboard".to_string();
                }
            });
        });
        
        ui.add_space(10.0);
        
        // Sync settings
        ui.group(|ui| {
            ui.label("Sync Settings");
            
            ui.horizontal(|ui| {
                ui.label("Sync interval:");
                let mut interval = self.config.sync_interval_minutes as f32;
                if ui.add(egui::Slider::new(&mut interval, 1.0..=60.0).suffix(" minutes")).changed() {
                    let mut config = (*self.config).clone();
                    if config.set_sync_interval(interval as u64).is_ok() {
                        // Config updated
                    }
                }
            });
        });
        
        ui.add_space(10.0);
        
        // About section
        ui.group(|ui| {
            ui.label("About");
            ui.label("OneDrive Ubuntu Client v1.0.0");
            ui.label("Built with Rust and egui");
            ui.label(format!("Config directory: {}", self.config.config_dir.display()));
        });
    }
    
    fn show_logs_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sync Logs");
        
        ui.separator();
        
        // Refresh cache every 5 seconds or on manual refresh
        let should_refresh = ui.button("Refresh Logs").clicked() || 
                           self.last_history_refresh.elapsed() > Duration::from_secs(5);
        
        if should_refresh {
            info!("Refreshing sync logs");
            // Try to refresh cache from sync manager
            if let Ok(history) = self.rt.block_on(async {
                if let Ok(sync_guard) = tokio::time::timeout(
                    Duration::from_millis(100),
                    self.sync_manager.lock()
                ).await {
                    sync_guard.get_sync_history(50).await
                } else {
                    Err(anyhow::anyhow!("Sync manager busy"))
                }
            }) {
                self.sync_history_cache = history;
                self.last_history_refresh = std::time::Instant::now();
            }
        }
        
        ui.add_space(10.0);
        
        // Show cached sync history
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.sync_history_cache.is_empty() {
                ui.label("No sync history yet");
                ui.label("Start a sync to see log entries here");
                
                // Show database path for debugging
                if let Some(config_dir) = dirs::config_dir() {
                    let db_path = config_dir.join("onedrive-ubuntu").join("sync.db");
                    ui.add_space(10.0);
                    ui.separator();
                    ui.label("Debug info:");
                    ui.label(format!("Database path: {}", db_path.display()));
                    
                    if db_path.exists() {
                        ui.colored_label(egui::Color32::GREEN, "✓ Database file exists");
                    } else {
                        ui.colored_label(egui::Color32::RED, "✗ Database file not found");
                    }
                }
            } else {
                ui.label(format!("Showing {} recent log entries:", self.sync_history_cache.len()));
                ui.separator();
                
                for entry in &self.sync_history_cache {
                    let timestamp = std::time::UNIX_EPOCH + Duration::from_secs(entry.timestamp);
                    let datetime = chrono::DateTime::<chrono::Utc>::from(timestamp);
                    let formatted_time = datetime.format("%Y-%m-%d %H:%M:%S UTC");
                    
                    let status_color = match entry.status.as_str() {
                        "success" => egui::Color32::GREEN,
                        "failed" => egui::Color32::RED,
                        _ => egui::Color32::GRAY,
                    };
                    
                    ui.horizontal(|ui| {
                        ui.label(format!("{}", formatted_time));
                        ui.colored_label(status_color, &entry.status.to_uppercase());
                        ui.label(&entry.action);
                        ui.label(&entry.file_path);
                    });
                    
                    if let Some(ref error) = entry.error {
                        ui.colored_label(egui::Color32::RED, format!("  Error: {}", error));
                    }
                    
                    ui.separator();
                }
            }
            
            // Always show last refresh time
            ui.add_space(10.0);
            ui.label(format!("Last refreshed: {:?} ago", self.last_history_refresh.elapsed()));
        });
    }
    
    fn authenticate(&mut self, ctx: &egui::Context) {
        info!("Starting authentication");
        self.status_message = "Opening browser for authentication...".to_string();
        
        let auth = self.auth.clone();
        let ctx = ctx.clone();
        
        // Start authentication in background
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async {
                let mut auth_guard = auth.lock().await;
                match auth_guard.authenticate().await {
                    Ok(_) => {
                        info!("Authentication successful - requesting GUI repaint");
                        ctx.request_repaint();
                    }
                    Err(e) => {
                        error!("Authentication failed: {}", e);
                        // Note: Status message will be updated by the main update loop
                        ctx.request_repaint();
                    }
                }
            });
        });
    }
    
    fn sign_out(&mut self) {
        let mut auth_guard = self.rt.block_on(async {
            self.auth.lock().await
        });
        if auth_guard.logout().is_ok() {
            self.user_info = None;
            self.drive_info = None;
            self.status_message = "Signed out successfully".to_string();
            info!("User signed out");
        }
    }
    
    fn start_manual_sync(&mut self) {
        info!("Starting manual sync from GUI");
        self.status_message = "Starting sync...".to_string();
        
        let sync_manager = self.sync_manager.clone();
        
        // Use the existing runtime instead of creating a new thread
        let _ = self.rt.spawn(async move {
            let mut sync_guard = sync_manager.lock().await;
            match sync_guard.sync().await {
                Ok(_) => {
                    info!("Manual sync completed successfully");
                }
                Err(e) => {
                    error!("Manual sync failed: {}", e);
                }
            }
        });
    }
    
    fn update_sync_folder(&mut self) {
        let new_path = std::path::PathBuf::from(&self.new_sync_folder);
        let mut config = (*self.config).clone();
        
        if config.update_sync_folder(new_path).is_ok() {
            self.status_message = "Sync folder updated successfully".to_string();
            info!("Sync folder updated to: {}", self.new_sync_folder);
        } else {
            self.status_message = "Failed to update sync folder".to_string();
            error!("Failed to update sync folder");
        }
    }
    
    fn show_setup_wizard_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                
                ui.heading("OneDrive Ubuntu Client Setup");
                ui.add_space(20.0);
                
                match self.setup_step {
                    SetupStep::Welcome => self.show_welcome_step(ui),
                    SetupStep::AzureInstructions => self.show_azure_instructions_step(ui),
                    SetupStep::ClientIdInput => self.show_client_id_input_step(ui),
                    SetupStep::Complete => self.show_complete_step(ui),
                }
            });
        });
    }
    
    fn show_welcome_step(&mut self, ui: &mut egui::Ui) {
        ui.label("Welcome to the OneDrive Ubuntu Client!");
        ui.add_space(20.0);
        
        ui.label("To get started, you need to create an Azure App Registration.");
        ui.label("This allows the client to securely connect to your OneDrive account.");
        ui.add_space(20.0);
        
        ui.label("Don't worry - we'll guide you through the process step by step.");
        ui.add_space(30.0);
        
        if ui.button("Get Started").clicked() {
            self.setup_step = SetupStep::AzureInstructions;
        }
    }
    
    fn show_azure_instructions_step(&mut self, ui: &mut egui::Ui) {
        ui.label("Step 1: Create Azure App Registration");
        ui.add_space(20.0);
        
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Follow these steps:");
                ui.add_space(10.0);
                
                ui.horizontal(|ui| {
                    ui.label("1.");
                    ui.vertical(|ui| {
                        ui.label("Go to the Azure Portal:");
                        if ui.link("https://portal.azure.com").clicked() {
                            let _ = open::that("https://portal.azure.com");
                        }
                    });
                });
                
                ui.label("2. Navigate to: Azure Active Directory → App registrations");
                ui.label("3. Click 'New registration'");
                ui.label("4. Fill in the registration form:");
                
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("• Name: OneDrive Ubuntu Client");
                        ui.label("• Account types: Accounts in any organizational directory and personal Microsoft accounts");
                        ui.label("• Redirect URI: Web → http://localhost:8080/callback");
                    });
                });
                
                ui.label("5. After creation, go to 'API permissions' and add:");
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("• Microsoft Graph → Delegated permissions → Files.ReadWrite.All");
                        ui.label("• Microsoft Graph → Delegated permissions → User.Read");
                    });
                });
                
                ui.label("6. Grant admin consent (if you're an admin)");
                ui.label("7. Go to the 'Overview' tab and copy the 'Application (client) ID'");
            });
        });
        
        ui.add_space(20.0);
        
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                self.setup_step = SetupStep::Welcome;
            }
            
            if ui.button("I've Created the App →").clicked() {
                self.setup_step = SetupStep::ClientIdInput;
            }
        });
    }
    
    fn show_client_id_input_step(&mut self, ui: &mut egui::Ui) {
        ui.label("Step 2: Enter Your Client ID");
        ui.add_space(20.0);
        
        ui.label("Paste the Application (client) ID from your Azure App Registration:");
        ui.add_space(10.0);
        
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Client ID:");
                ui.text_edit_singleline(&mut self.client_id_input);
                
                if !self.client_id_input.is_empty() {
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::GRAY, 
                        format!("Example: {}", "12345678-1234-1234-1234-123456789abc"));
                }
            });
        });
        
        ui.add_space(20.0);
        
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                self.setup_step = SetupStep::AzureInstructions;
            }
            
            let is_valid_uuid = self.is_valid_client_id(&self.client_id_input);
            ui.add_enabled_ui(is_valid_uuid, |ui| {
                if ui.button("Save Configuration →").clicked() {
                    if self.save_client_id() {
                        self.setup_step = SetupStep::Complete;
                    }
                }
            });
        });
        
        if !self.client_id_input.is_empty() && !self.is_valid_client_id(&self.client_id_input) {
            ui.add_space(10.0);
            ui.colored_label(egui::Color32::RED, "Please enter a valid UUID format client ID");
        }
    }
    
    fn show_complete_step(&mut self, ui: &mut egui::Ui) {
        ui.label("🎉 Setup Complete!");
        ui.add_space(20.0);
        
        ui.label("Your OneDrive Ubuntu Client configuration has been saved.");
        ui.add_space(10.0);
        
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Configuration saved:");
                ui.label(format!("• Client ID: {}", self.client_id_input));
                ui.label(format!("• Redirect URI: http://localhost:8080/callback"));
                ui.label(format!("• Sync Folder: {}", self.config.sync_folder.display()));
            });
        });
        
        ui.add_space(20.0);
        
        // Show restart message
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.colored_label(egui::Color32::YELLOW, "⚠️ Application Restart Required");
                ui.label("Please close and restart the application to use the new configuration.");
                ui.add_space(10.0);
                ui.label("After restart, you'll be able to authenticate with Microsoft and start syncing.");
            });
        });
        
        ui.add_space(20.0);
        
        if ui.button("Close Application").clicked() {
            std::process::exit(0);
        }
    }
    
    fn is_valid_client_id(&self, client_id: &str) -> bool {
        // Basic UUID format validation
        client_id.len() == 36 && 
        client_id.chars().enumerate().all(|(i, c)| {
            match i {
                8 | 13 | 18 | 23 => c == '-',
                _ => c.is_ascii_hexdigit(),
            }
        })
    }
    
    fn save_client_id(&mut self) -> bool {
        use std::fs;
        
        // Create config directory if it doesn't exist
        if let Err(e) = fs::create_dir_all(&self.config.config_dir) {
            error!("Failed to create config directory: {}", e);
            return false;
        }
        
        // Create new config with the client ID using the proper config method
        let config_content = format!(
            r#"client_id = "{}"
redirect_uri = "http://localhost:8080/callback"
sync_folder = "{}"
sync_interval_minutes = {}
auto_start = {}
minimize_to_tray = {}
notifications = {}
debug_logging = {}
"#,
            self.client_id_input,
            self.config.sync_folder.display(),
            self.config.sync_interval_minutes,
            self.config.auto_start,
            self.config.minimize_to_tray,
            self.config.notifications,
            self.config.debug_logging
        );
        
        match fs::write(&self.config.config_file, config_content) {
            Ok(_) => {
                info!("Configuration saved successfully");
                true
            }
            Err(e) => {
                error!("Failed to save configuration: {}", e);
                false
            }
        }
    }
}
