# Changelog

All notable changes to the OneDrive Ubuntu Client will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2024-07-30

### Added
- Initial release of OneDrive Ubuntu Client
- Complete OAuth 2.0 authentication with Microsoft Graph API
- GUI interface built with Rust and egui framework
- Real-time bidirectional file synchronization
- Support for both personal and business Microsoft accounts
- System tray integration with status indicators
- Desktop integration with autostart capabilities
- SQLite database for sync state management
- Comprehensive error handling and logging
- PKCE flow for secure public client authentication
- Chunked file uploads for large files
- Real-time file system monitoring with watchdog
- Professional documentation and troubleshooting guides

### Features
- **Secure Authentication**: OAuth 2.0 with PKCE flow, no client secrets
- **Cross-Platform Accounts**: Works with personal and business OneDrive accounts
- **GUI Interface**: Clean, modern interface with real-time status updates
- **Automatic Sync**: Configurable sync intervals with manual sync option
- **Conflict Resolution**: Intelligent handling of file conflicts with timestamps
- **System Integration**: Native Ubuntu desktop integration and system tray
- **Robust Error Handling**: Comprehensive error reporting and recovery
- **Performance Optimized**: Written in Rust for memory safety and speed

### Security
- OAuth 2.0 with PKCE (Proof Key for Code Exchange) for secure authentication
- No client secrets stored locally
- Secure token storage using system capabilities
- All communications over HTTPS/TLS
- No password storage - token-based authentication only

### Documentation
- Comprehensive README with installation and usage instructions
- Detailed installation guide (INSTALL.md)
- Extensive troubleshooting guide (TROUBLESHOOTING.md)
- Professional documentation without decorative elements
- Complete Azure App Registration setup instructions

### Technical Details
- **Language**: Rust 2021 edition
- **GUI Framework**: egui/eframe 0.27
- **Database**: SQLite with rusqlite 0.31
- **HTTP Client**: reqwest 0.11 with async support
- **Authentication**: oauth2 4.4 with custom public client implementation
- **Logging**: tracing framework with configurable levels
- **File Monitoring**: notify 6.1 for real-time file system events

### Platform Support
- Ubuntu 18.04 LTS and later
- Compatible with other Debian-based Linux distributions
- X11 and Wayland display server support
- GTK 3 integration for native look and feel

### Configuration
- TOML-based configuration file
- Customizable sync folder location
- Configurable sync intervals
- Optional debug logging
- System tray and notification preferences
- Autostart configuration options
