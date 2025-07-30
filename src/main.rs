use anyhow::Result;
use eframe::egui;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, error};

mod config;
mod auth;
mod api;
mod sync;
mod tray;
mod gui;

use config::Config;
use auth::AuthManager;
use api::OneDriveAPI;
use sync::SyncManager;
use gui::OneDriveApp;
use tray::TrayManager;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("Starting OneDrive Ubuntu Client v1.0.0");

    // Check for single instance
    if !check_single_instance()? {
        info!("Another instance is already running");
        return Ok(());
    }

    // Check if we should start async or sync mode
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() > 1 {
        match args[1].as_str() {
            "--tray-only" => {
                // Start in tray-only mode (for autostart) - needs async
                run_tray_mode()
            }
            "--setup-autostart" => {
                // Setup autostart
                info!("Setting up autostart");
                setup_autostart()?;
                println!("Autostart configured successfully!");
                Ok(())
            }
            "--help" => {
                println!("OneDrive Ubuntu Client v1.0.0");
                println!("Usage:");
                println!("  onedrive-ubuntu                    # Run GUI application");
                println!("  onedrive-ubuntu --tray-only        # Run in system tray only");
                println!("  onedrive-ubuntu --setup-autostart  # Setup autostart");
                println!("  onedrive-ubuntu --help             # Show this help");
                Ok(())
            }
            _ => {
                error!("Unknown argument: {}", args[1]);
                Ok(())
            }
        }
    } else {
        // Start GUI application (sync mode)
        run_gui_mode()
    }
}

#[tokio::main]
async fn run_tray_mode() -> Result<()> {
    // Initialize configuration
    let config = Arc::new(Config::new()?);
    info!("Configuration loaded");

    // Initialize authentication
    let auth = Arc::new(Mutex::new(AuthManager::new(config.clone())?));
    
    // Initialize OneDrive API client
    let api = Arc::new(OneDriveAPI::new(auth.clone()));
    
    // Initialize sync manager
    let sync_manager = Arc::new(Mutex::new(SyncManager::new(config.clone(), api.clone())?));

    info!("Starting in tray-only mode");
    let tray = TrayManager::new(config.clone(), auth.clone(), sync_manager.clone())?;
    tray.run().await?;
    
    Ok(())
}

fn run_gui_mode() -> Result<()> {
    // Initialize configuration
    let config = Arc::new(Config::new()?);
    info!("Configuration loaded");

    // Initialize authentication
    let auth = Arc::new(Mutex::new(AuthManager::new(config.clone())?));
    
    // Initialize OneDrive API client
    let api = Arc::new(OneDriveAPI::new(auth.clone()));
    
    // Initialize sync manager
    let sync_manager = Arc::new(Mutex::new(SyncManager::new(config.clone(), api.clone())?));

    // Start GUI application
    info!("Starting GUI application");
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([600.0, 400.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    let app = OneDriveApp::new(config, auth, sync_manager);
    
    let _ = eframe::run_native(
        "OneDrive Ubuntu Client",
        options,
        Box::new(|_cc| Box::new(app)),
    );
    
    Ok(())
}

fn load_icon() -> Arc<egui::IconData> {
    // Create a simple blue icon with "OD" text
    let icon_size = 32;
    let mut icon_data = vec![0u8; icon_size * icon_size * 4]; // RGBA
    
    // Fill with blue background
    for i in (0..icon_data.len()).step_by(4) {
        icon_data[i] = 0;     // R
        icon_data[i + 1] = 100; // G
        icon_data[i + 2] = 200; // B
        icon_data[i + 3] = 255; // A
    }
    
    Arc::new(egui::IconData {
        rgba: icon_data,
        width: icon_size as u32,
        height: icon_size as u32,
    })
}

fn setup_autostart() -> Result<()> {
    use std::fs;
    
    let home_dir = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let autostart_dir = home_dir.join(".config").join("autostart");
    
    // Create autostart directory
    fs::create_dir_all(&autostart_dir)?;
    
    // Get the current executable path
    let exe_path = std::env::current_exe()?;
    
    let desktop_entry = format!(
        r#"[Desktop Entry]
Type=Application
Name=OneDrive Ubuntu Client
Comment=Synchronize files with Microsoft OneDrive
Exec={} --tray-only
Icon=folder-cloud
StartupNotify=false
NoDisplay=true
Hidden=false
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-Delay=10
Categories=Network;FileTransfer;
"#,
        exe_path.display()
    );
    
    let desktop_file = autostart_dir.join("onedrive-ubuntu.desktop");
    fs::write(&desktop_file, desktop_entry)?;
    
    info!("Autostart desktop entry created: {}", desktop_file.display());
    Ok(())
}

fn check_single_instance() -> Result<bool> {
    use std::fs;
    use std::process;
    
    let lock_file = dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .unwrap_or_else(|| std::env::temp_dir())
        .join("onedrive-ubuntu.lock");
    
    // Try to read existing lock file
    if lock_file.exists() {
        if let Ok(pid_str) = fs::read_to_string(&lock_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                // Check if process is still running
                if process_exists(pid) {
                    println!("OneDrive Ubuntu Client is already running (PID: {})", pid);
                    return Ok(false);
                } else {
                    // Remove stale lock file
                    let _ = fs::remove_file(&lock_file);
                }
            }
        }
    }
    
    // Create new lock file with current PID
    let current_pid = process::id();
    fs::write(&lock_file, current_pid.to_string())?;
    
    // Set up cleanup on exit
    let lock_file_clone = lock_file.clone();
    ctrlc::set_handler(move || {
        let _ = fs::remove_file(&lock_file_clone);
        std::process::exit(0);
    })?;
    
    Ok(true)
}

fn process_exists(pid: u32) -> bool {
    use std::process::Command;
    
    // Use `kill -0` to check if process exists (Linux/Unix)
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
