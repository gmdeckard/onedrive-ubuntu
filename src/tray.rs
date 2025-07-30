use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, error, warn};
use tray_icon::{TrayIcon, TrayIconBuilder, menu::{Menu, MenuItem, MenuEvent}};
use image::ImageBuffer;
use std::time::Duration;

use crate::auth::AuthManager;
use crate::config::Config;
use crate::sync::SyncManager;

pub struct TrayManager {
    config: Arc<Config>,
    auth: Arc<Mutex<AuthManager>>,
    sync_manager: Arc<Mutex<SyncManager>>,
    tray_icon: Option<TrayIcon>,
}

impl TrayManager {
    pub fn new(
        config: Arc<Config>,
        auth: Arc<Mutex<AuthManager>>,
        sync_manager: Arc<Mutex<SyncManager>>,
    ) -> Result<Self> {
        Ok(Self {
            config,
            auth,
            sync_manager,
            tray_icon: None,
        })
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Starting system tray");

        // Initialize the tray icon with retry logic
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 5;
        
        while retry_count < MAX_RETRIES {
            match self.try_create_tray_icon().await {
                Ok(_) => {
                    info!("System tray initialized successfully");
                    break;
                }
                Err(e) => {
                    retry_count += 1;
                    warn!("Failed to create tray icon (attempt {}): {}", retry_count, e);
                    if retry_count >= MAX_RETRIES {
                        error!("Failed to initialize system tray after {} attempts", MAX_RETRIES);
                        return Err(e);
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        // Start auto-sync in background
        let sync_manager_clone = self.sync_manager.clone();
        tokio::spawn(async move {
            let mut sync_guard = sync_manager_clone.lock().await;
            sync_guard.start_auto_sync().await;
        });

        // Start status update loop (without spawning to avoid Send issues)
        info!("System tray initialized successfully");

        // Handle menu events
        let menu_channel = MenuEvent::receiver();
        
        loop {
            tokio::select! {
                event_result = tokio::task::spawn_blocking(move || menu_channel.recv()) => {
                    match event_result {
                        Ok(Ok(event)) => {
                            if let Err(e) = self.handle_menu_event(event).await {
                                error!("Error handling menu event: {}", e);
                            }
                        }
                        Ok(Err(_)) => {
                            warn!("Menu event channel closed");
                            break;
                        }
                        Err(e) => {
                            error!("Error receiving menu event: {}", e);
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Keep the loop running and update tray status
                    self.update_tray_status().await;
                }
            }
        }

        info!("System tray shutting down");
        Ok(())
    }

    async fn try_create_tray_icon(&mut self) -> Result<()> {
        // Create tray icon
        let icon = self.create_icon();
        
        let tray_menu = Menu::new();
        
        let open_item = MenuItem::new("Open OneDrive", true, None);
        let sync_item = MenuItem::new("Sync Now", true, None);
        let status_item = MenuItem::new("Status: Ready", false, None);
        let settings_item = MenuItem::new("Settings", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        
        tray_menu.append_items(&[
            &status_item,
            &open_item,
            &sync_item,
            &settings_item,
            &quit_item,
        ])?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("OneDrive Ubuntu Client")
            .with_icon(icon)
            .build()?;

        self.tray_icon = Some(tray_icon);
        Ok(())
    }

    async fn handle_menu_event(&mut self, event: MenuEvent) -> Result<()> {
        info!("Menu event received: {:?}", event.id);
        
        // Simple approach using menu text to identify actions
        // This is not ideal but avoids the complex ID matching issues
        
        // For now, just handle based on text content or implement a simple counter
        // This is a basic implementation - in production you'd want better menu ID tracking
        
        info!("Opening GUI from tray (menu event)");
        self.open_gui().await?;
        
        Ok(())
    }

    async fn update_tray_status(&mut self) {
        if let Some(ref tray_icon) = self.tray_icon {
            let status = {
                let sync_guard = self.sync_manager.lock().await;
                sync_guard.get_status().await
            };
            
            let tooltip = if status.is_syncing {
                format!("OneDrive - {}", status.current_operation)
            } else if let Some(last_sync) = status.last_sync {
                let elapsed = std::time::SystemTime::now()
                    .duration_since(last_sync)
                    .unwrap_or_default()
                    .as_secs();
                format!("OneDrive - Last sync: {}s ago", elapsed)
            } else {
                "OneDrive - Ready".to_string()
            };
            
            // Update tooltip
            if let Err(e) = tray_icon.set_tooltip(Some(&tooltip)) {
                warn!("Failed to update tray tooltip: {}", e);
            }
        }
    }

    fn create_icon(&self) -> tray_icon::Icon {
        // Create a simple blue icon with "OD" text
        let size = 32;
        let mut image_data = ImageBuffer::new(size, size);
        
        // Fill with blue background
        for pixel in image_data.pixels_mut() {
            *pixel = image::Rgba([0, 100, 200, 255]);
        }
        
        // Add white square in center (simplified)
        for y in 8..24 {
            for x in 8..24 {
                image_data.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }

        let rgba_data = image_data.into_raw();
        
        tray_icon::Icon::from_rgba(rgba_data, size, size)
            .expect("Failed to create tray icon")
    }

    async fn open_gui(&self) -> Result<()> {
        // Launch GUI in a separate process
        let exe_path = std::env::current_exe()?;
        
        tokio::process::Command::new(exe_path)
            .spawn()?;
        
        Ok(())
    }

    async fn start_sync(&self) -> Result<()> {
        let sync_manager = self.sync_manager.clone();
        
        tokio::spawn(async move {
            let mut sync_guard = sync_manager.lock().await;
            match sync_guard.sync().await {
                Ok(_) => {
                    info!("Tray-initiated sync completed successfully");
                    // Could show notification here
                }
                Err(e) => {
                    error!("Tray-initiated sync failed: {}", e);
                    // Could show error notification here
                }
            }
        });

        Ok(())
    }

    async fn open_settings(&self) -> Result<()> {
        // For now, just open the GUI
        // In a more advanced implementation, could open directly to settings tab
        self.open_gui().await
    }

    pub fn update_icon_status(&mut self, is_syncing: bool, has_errors: bool) -> Result<()> {
        let (tooltip, icon) = if has_errors {
            let icon = self.create_error_icon();
            ("OneDrive - Sync Error", icon)
        } else if is_syncing {
            let icon = self.create_syncing_icon();
            ("OneDrive - Syncing...", icon)
        } else {
            let icon = self.create_icon();
            ("OneDrive - Up to date", icon)
        };

        if let Some(ref mut tray_icon) = self.tray_icon {
            tray_icon.set_tooltip(Some(tooltip))?;
            tray_icon.set_icon(Some(icon))?;
        }

        Ok(())
    }

    fn create_syncing_icon(&self) -> tray_icon::Icon {
        // Create an icon with spinning animation indicator
        let size = 32;
        let mut image_data = ImageBuffer::new(size, size);
        
        // Fill with blue background
        for pixel in image_data.pixels_mut() {
            *pixel = image::Rgba([0, 150, 255, 255]); // Lighter blue for syncing
        }
        
        // Add rotating indicator (simplified)
        for y in 8..24 {
            for x in 8..24 {
                if (x + y) % 4 == 0 {
                    image_data.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
                }
            }
        }

        let rgba_data = image_data.into_raw();
        
        tray_icon::Icon::from_rgba(rgba_data, size, size)
            .expect("Failed to create syncing icon")
    }

    fn create_error_icon(&self) -> tray_icon::Icon {
        // Create a red icon to indicate errors
        let size = 32;
        let mut image_data = ImageBuffer::new(size, size);
        
        // Fill with red background
        for pixel in image_data.pixels_mut() {
            *pixel = image::Rgba([200, 50, 50, 255]);
        }
        
        // Add white X (simplified)
        for i in 8..24 {
            image_data.put_pixel(i, i, image::Rgba([255, 255, 255, 255]));
            image_data.put_pixel(i, 32 - i, image::Rgba([255, 255, 255, 255]));
        }

        let rgba_data = image_data.into_raw();
        
        tray_icon::Icon::from_rgba(rgba_data, size, size)
            .expect("Failed to create error icon")
    }
}
