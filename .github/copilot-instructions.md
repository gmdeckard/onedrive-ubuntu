<!-- Use this file to provide workspace-specific custom instructions to Copilot. For more details, visit https://code.visualstudio.com/docs/copilot/copilot-customization#_use-a-githubcopilotinstructionsmd-file -->

# OneDrive Ubuntu Client - Copilot Instructions

## Project Overview
This is a complete OneDrive synchronization client for Ubuntu Linux that provides both command-line and graphical interfaces. The project uses the Microsoft Graph API for secure file synchronization.

## Key Components

### Core Modules
- `onedrive_client.py` - Main synchronization engine and CLI interface
- `onedrive_gui.py` - Tkinter-based graphical user interface
- `install.sh` - Automated installation script
- `setup.py` - Python package configuration

### Architecture
- **OneDriveConfig**: Configuration management and file handling
- **OneDriveAuth**: OAuth 2.0 authentication with Microsoft Graph API
- **OneDriveAPI**: Microsoft Graph API wrapper for file operations
- **OneDriveSyncManager**: Main synchronization logic and file monitoring
- **SyncDatabase**: Local database for tracking file sync state
- **FileWatcher**: Real-time file system monitoring using watchdog

## Code Style Guidelines
- Follow PEP 8 style guidelines
- Use type hints for all function parameters and return values
- Add comprehensive docstrings to all public methods
- Handle exceptions gracefully with appropriate logging
- Use pathlib.Path for file system operations
- Prefer f-strings for string formatting

## Authentication Flow
- Uses MSAL (Microsoft Authentication Library) for OAuth 2.0
- Device flow authentication for headless systems
- Secure token storage using system keyring
- Automatic token refresh handling

## File Synchronization Logic
- Bidirectional sync between local folder and OneDrive
- File change detection using MD5 hashes
- Chunked uploads for large files (>4MB)
- Real-time monitoring with watchdog
- Conflict resolution with rename strategy

## Error Handling Patterns
- Log all errors with appropriate severity levels
- Graceful degradation for network issues
- Retry logic for transient failures
- User-friendly error messages in GUI

## Configuration Management
- JSON-based configuration files in ~/.config/onedrive-ubuntu/
- Environment variable support for headless operation
- Secure storage of sensitive data
- Validation of configuration parameters

## GUI Development
- Use tkinter for cross-platform compatibility
- Implement threading for non-blocking operations
- Provide real-time status updates
- Follow platform UI conventions

## Testing Considerations
- Mock external API calls for unit tests
- Test file system operations with temporary directories
- Validate configuration loading and saving
- Test authentication flows with mock responses

## Dependencies
- requests: HTTP client for API calls
- msal: Microsoft authentication library
- watchdog: File system monitoring
- tkinter: GUI framework (system package)
- keyring: Secure credential storage
- cryptography: Encryption support

## Security Best Practices
- Never log sensitive information (tokens, passwords)
- Use secure token storage mechanisms
- Validate all user inputs
- Implement proper file permissions
- Use HTTPS for all network communications

## Common Patterns
- Use context managers for file operations
- Implement proper cleanup in exception handlers
- Use threading for long-running operations
- Provide progress feedback for user operations
- Log important state changes for debugging
