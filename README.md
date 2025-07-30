# OneDrive Ubuntu Client

A modern, secure OneDrive synchronization client for Ubuntu Linux with GUI interface.

## Quick Start

**New to OneDrive Ubuntu Client?** Follow these essential steps:

1. **[Create Azure App Registration](#prerequisites-azure-app-registration)** (Required - 5 minutes)
2. **[Download and install .deb package](#quick-install-recommended)** (2 minutes)
3. **[Run setup wizard](#first-time-setup)** (Enter your Azure Client ID)
4. **Start syncing!** 

> **⚠️ Important**: You must create your own Azure app registration. The default client ID is for demonstration only and may not work reliably.

## Features

- **Secure Authentication**: OAuth 2.0 with Microsoft Graph API using PKCE flow
- **GUI Interface**: Clean, user-friendly graphical interface built with Rust and egui
- **Cross-Platform Account Support**: Works with both personal Microsoft accounts and business/organization accounts
- **Real-time Synchronization**: Bidirectional file sync with conflict detection
- **System Integration**: Desktop entries, autostart, and system tray support
- **Robust Error Handling**: Comprehensive error reporting and recovery mechanisms

## System Requirements

- Ubuntu 18.04 or later (or compatible Debian-based distributions)
- Internet connection
- Modern web browser for authentication
- At least 50MB free disk space for the application
- GTK 3.0+ (usually pre-installed on Ubuntu Desktop)

## Installation

### Quick Install (Recommended)

1. **Download the latest .deb package**:
   ```bash
   wget https://github.com/gmdeckard/onedrive-ubuntu/releases/latest/download/onedrive-ubuntu_1.0.0-1_amd64.deb
   ```

2. **Install the package**:
   ```bash
   sudo dpkg -i onedrive-ubuntu_1.0.0-1_amd64.deb
   sudo apt-get install -f  # Fix any dependency issues if needed
   ```

3. **Launch from Applications menu** or run:
   ```bash
   onedrive-ubuntu
   ```

> **Note**: If the download link above doesn't work, visit the [Releases page](https://github.com/gmdeckard/onedrive-ubuntu/releases) to download the latest version manually.

### Prerequisites: Azure App Registration

**Important**: Before using the application, you need to create your own Azure app registration for security and reliability.

#### Step 1: Create Azure App Registration

1. **Go to Azure Portal**:
   - Visit [Azure Portal](https://portal.azure.com)
   - Sign in with your Microsoft account

2. **Navigate to App Registrations**:
   - Search for "App registrations" in the top search bar
   - Click on "App registrations" service

3. **Create New Registration**:
   - Click "New registration"
   - Enter application details:
     - **Name**: `OneDrive Ubuntu Client` (or your preferred name)
     - **Supported account types**: Select "Personal Microsoft accounts only" for personal OneDrive, or "Accounts in any organizational directory and personal Microsoft accounts" for both business and personal accounts
     - **Redirect URI**: Leave blank for now
   - Click "Register"

4. **Configure Authentication**:
   - In your new app registration, go to "Authentication" in the left menu
   - Click "Add a platform"
   - Select "Mobile and desktop applications"
   - Check the box for "http://localhost:8080/callback"
   - Click "Configure"

5. **Set as Public Client**:
   - Still in Authentication section
   - Scroll down to "Advanced settings"
   - Set "Allow public client flows" to "Yes"
   - Click "Save"

6. **Copy Your Client ID**:
   - Go to "Overview" in the left menu
   - Copy the "Application (client) ID" - you'll need this during setup

#### Step 2: API Permissions (Optional but Recommended)

1. **Go to API Permissions**:
   - Click "API permissions" in the left menu
   - Click "Add a permission"
   - Select "Microsoft Graph"
   - Choose "Delegated permissions"

2. **Add Required Permissions**:
   - Search for and add these permissions:
     - `Files.ReadWrite` - Read and write access to user files
     - `Files.ReadWrite.All` - Read and write access to all files user can access
     - `User.Read` - Sign in and read user profile
   - Click "Add permissions"

3. **Grant Admin Consent** (if using business account):
   - Click "Grant admin consent for [Your Organization]"
   - Confirm the action

### Alternative Installation Methods

**Option 1: Download .deb manually**:
- Go to [Releases](https://github.com/gmdeckard/onedrive-ubuntu/releases)
- Download the latest `onedrive-ubuntu_x.x.x-x_amd64.deb` file
- Double-click to install via Software Center, or use `sudo dpkg -i filename.deb`

**Option 2: Build from source** (for developers):
```bash
# Install cargo-deb for packaging
cargo install cargo-deb

# Clone and build
git clone https://github.com/gmdeckard/onedrive-ubuntu.git
cd onedrive-ubuntu
cargo deb

# Install the generated package
sudo dpkg -i target/debian/onedrive-ubuntu_1.0.0-1_amd64.deb
```

## Usage

### First Time Setup

**Prerequisites**: Make sure you've completed the Azure App Registration steps above and installed the .deb package.

1. **Launch the application**:
   - Open from Applications menu (search for "OneDrive")
   - Or run from terminal: `onedrive-ubuntu`

2. **Configure Azure App** (First Time Only):
   - When you first run the app, you'll see a setup wizard
   - Enter the Client ID from your Azure app registration
   - Follow the on-screen instructions

3. **Authenticate with Microsoft**:
   - Click "Sign In with Microsoft" in the GUI
   - Your browser will open automatically
   - Sign in with your Microsoft account
   - Grant permissions to the application
   - Return to the app - you're now authenticated!

4. **Choose your sync folder** (optional):
   - Go to Settings tab
   - Change the sync folder path if desired
   - Default is `~/OneDrive`

5. **Start syncing**:
   - Files will sync automatically every 5 minutes
   - Or click "Sync Now" for manual sync
   - Monitor progress in the Status tab
   - The app will start automatically when you log in

### Running Modes

**GUI Mode** (default):
```bash
onedrive-ubuntu
```

**Tray Only Mode** (runs in background):
```bash
onedrive-ubuntu --tray-only
```

**Check Status**:
```bash
onedrive-ubuntu --status
```

**Help**:
```bash
onedrive-ubuntu --help
```

### Uninstalling

To completely remove OneDrive Ubuntu Client:

```bash
sudo apt remove onedrive-ubuntu
# Remove configuration files (optional):
rm -rf ~/.config/onedrive-ubuntu
```

## Configuration

Configuration is stored in `~/.config/onedrive-ubuntu/config.toml`:

```toml
client_id = "your-azure-app-client-id-here"
redirect_uri = "http://localhost:8080/callback"
sync_folder = "/home/username/OneDrive"
sync_interval_minutes = 5
auto_start = true
minimize_to_tray = true
notifications = true
debug_logging = false
```

**Important Notes**:
- Replace `your-azure-app-client-id-here` with your actual Azure App Registration Client ID
- The default client ID `14d82eec-204b-4c2f-b7e8-296a70dab67e` is for demonstration only
- Using your own Azure app registration ensures security and avoids rate limits

### File Locations

- **Configuration**: `~/.config/onedrive-ubuntu/config.toml`
- **Authentication tokens**: `~/.config/onedrive-ubuntu/tokens.json`
- **Sync database**: `~/.config/onedrive-ubuntu/sync.db`
- **Logs**: `~/.config/onedrive-ubuntu/onedrive.log`
- **Autostart**: `~/.config/autostart/onedrive-ubuntu.desktop`

## How It Works

### Authentication
- Uses Microsoft's OAuth 2.0 device code flow
- Stores encrypted tokens securely on your system
- Automatically refreshes access tokens
- No passwords stored locally

### Synchronization
- Monitors local folder for changes using filesystem events
- Compares file hashes to detect modifications
- Uploads new/modified local files to OneDrive
- Downloads new/modified OneDrive files locally
- Maintains sync state database for conflict resolution

### Security
- All communication uses HTTPS/TLS encryption
- Tokens are stored securely using system keyring
- No sensitive data transmitted in logs
- Memory-safe Rust implementation prevents security vulnerabilities

## System Requirements

- **OS**: Ubuntu 18.04+ (or any modern Linux distribution)
- **Architecture**: x86_64 (AMD64)
- **Memory**: 50MB RAM minimum
- **Storage**: 10MB for application + sync folder space
- **Network**: Internet connection for OneDrive access

## Troubleshooting

### Authentication Issues

**"Invalid client ID" or AADSTS errors**:
- Verify your Azure app registration Client ID is correct
- Ensure the app is configured as a "Public client"
- Check that redirect URI `http://localhost:8080/callback` is configured
- Make sure "Allow public client flows" is set to "Yes"

**"Insufficient privileges" error**:
- Check your Azure app has the required API permissions:
  - `Files.ReadWrite`
  - `Files.ReadWrite.All` 
  - `User.Read`
- For business accounts, ensure admin consent is granted

**"Browser doesn't open"**:
```bash
# Manually open the authentication URL shown in terminal
# Or install a default browser:
sudo apt install firefox
```

**"Authentication failed"**:
- Check internet connection
- Ensure system time is correct
- Try clearing tokens: `rm ~/.config/onedrive-ubuntu/tokens.json`
- Verify your Azure app registration is active (not expired)

### Sync Issues

**"Sync not working"**:
```bash
# Check logs for errors
onedrive-ubuntu
# Go to Logs tab, or check:
cat ~/.config/onedrive-ubuntu/onedrive.log
```

**"Permission denied"**:
```bash
# Ensure sync folder is writable
chmod 755 ~/OneDrive
```

**"Files not syncing"**:
- Check available disk space
- Verify network connectivity
- Try manual sync in GUI

### Performance Issues

**"High CPU usage"**:
- Reduce sync frequency in Settings
- Exclude large files/folders if needed
- Check for filesystem permission issues

**"Slow sync"**:
- Large files take time to upload/download
- Check network bandwidth
- OneDrive has rate limits

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

1. **Clone the repository**:
   ```bash
   git clone https://github.com/gmdeckard/onedrive-ubuntu.git
   cd onedrive-ubuntu
   ```

2. **Install development dependencies**:
   ```bash
   sudo apt install build-essential pkg-config libssl-dev libgtk-3-dev libayatana-appindicator3-dev
   # For .deb packaging:
   sudo apt install cargo-deb
   ```

3. **Run in development mode**:
   ```bash
   cargo run
   ```

4. **Run tests**:
   ```bash
   cargo test
   ```

5. **Build .deb package**:
   ```bash
   cargo deb
   # Package will be created in target/debian/
   ```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- **Microsoft Graph API** for OneDrive integration
- **Rust ecosystem** for excellent crates and tooling
- **egui** for the modern GUI framework
- **Ubuntu community** for testing and feedback

## Support

- **Issues**: [GitHub Issues](https://github.com/gmdeckard/onedrive-ubuntu/issues)
- **Discussions**: [GitHub Discussions](https://github.com/gmdeckard/onedrive-ubuntu/discussions)
- **Email**: onedrive-ubuntu@example.com

---


