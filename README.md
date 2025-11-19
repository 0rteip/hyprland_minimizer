# Hyprland Minimizer

A lightweight system tray daemon for Hyprland that lets you minimize applications to the tray using special workspaces.

## Overview

Hyprland Minimizer creates persistent tray icons for your applications, allowing you to toggle their visibility with a single click or keyboard shortcut. Applications are moved to special (hidden) workspaces when minimized, keeping your main workspaces clean.

### Key Features

- **Per-app daemons** — One lightweight background process per managed application
- **Configurable apps** — Define any application via simple TOML config
- **Auto-launch** — Automatically starts applications if they're not running
- **Tray integration** — Full StatusNotifier protocol support (Waybar, etc.)
- **Smart detection** — Intelligent launch detection with configurable timeout
- **Single instance** — Prevents duplicate daemons with PID file locking

---

## Installation

### Build from source

```bash
git clone https://github.com/Simon-Martens/hyprland-minimizer
cd hyprland-minimizer
cargo build --release
sudo cp target/release/hyprland-minimizer /usr/local/bin/
```

### Copy example config (optional)

```bash
mkdir -p ~/.config/hyprland-minimizer
cp config.example.toml ~/.config/hyprland-minimizer/config.toml
```

A default config will be created automatically on first run if none exists.

---

## Configuration

Edit `~/.config/hyprland-minimizer/config.toml` to define your applications:

```toml
[apps.app_id]
name = "Display Name"
class = "window-class"               # Use: hyprctl clients | grep class
command = ["command", "arg1", "arg2"]
icon = "icon-name"                   # Optional: system icon name
notify_name = "notification-id"      # Optional: for desktop notifications
launch_in_background = false         # Optional: start hidden (default: false)
launch_timeout = 10                  # Optional: detection timeout in seconds (default: 10)
```

### Example: Firefox Web Apps

```toml
[apps.whatsapp]
name = "WhatsApp"
class = "whatsapp"
icon = "whatsapp"
command = ["firefox", "--name=whatsapp", "--new-window", "https://web.whatsapp.com/"]
launch_timeout = 15

[apps.gmail]
name = "Gmail"
class = "gmail"
icon = "gmail"
command = ["firefox", "--name=gmail", "--new-window", "https://mail.google.com/"]
```

### Example: Native Apps

```toml
[apps.spotify]
name = "Spotify"
class = "Spotify"
icon = "spotify"
command = ["spotify"]
launch_in_background = true

[apps.discord]
name = "Discord"
class = "discord"
icon = "discord"
command = ["discord", "--start-minimized"]
```

> **Tip:** Find window classes with `hyprctl clients | grep -i class`

---

## Usage

### Basic Commands

Start or toggle an application:

```bash
hyprland-minimizer <app_id>
```

Examples:

```bash
hyprland-minimizer whatsapp
hyprland-minimizer spotify
hyprland-minimizer discord
```

### Behavior

**First invocation:**

- Launches the application (if not running)
- Creates a system tray icon
- Starts a persistent daemon process
- Moves window to special workspace (if `launch_in_background = true`)

**Subsequent invocations:**

- Toggles window between current workspace and special workspace
- Signals existing daemon (no new process created)
- Fast and lightweight

### Tray Icon Interactions

- **Left click** — Toggle window visibility
- **Middle click** — Close the application
- **Right click** — Open context menu
  - Toggle window
  - Restore to original workspace
  - Close application

---

## Hyprland Integration

Add keybindings to your `~/.config/hypr/hyprland.conf`:

```conf
# Toggle applications with Super key
bind = SUPER, W, exec, hyprland-minimizer whatsapp
bind = SUPER, M, exec, hyprland-minimizer gmail
bind = SUPER, S, exec, hyprland-minimizer spotify
bind = SUPER, D, exec, hyprland-minimizer discord
```

---

## How It Works

1. **Window Management**: Applications are moved to special workspaces (negative workspace IDs in Hyprland)
2. **IPC**: Uses `hyprctl` commands to control window positions and states
3. **Daemon Communication**: UNIX signals (`SIGUSR1`) for efficient inter-process communication
4. **Tray Protocol**: Implements DBus StatusNotifier for system tray integration
5. **Process Locking**: PID files ensure only one daemon runs per application

---

## Troubleshooting

### Application not detected after launch

**Symptom**: "Failed to find window with class 'xyz' after launching"

**Solutions**:
- Verify the window class: `hyprctl clients | grep -i "class"`
- Increase `launch_timeout` in your config (slow apps may need 15-30 seconds)
- Check that the command launches correctly: run it manually first

### Tray icon not showing

**Solutions**:
- Ensure you have a StatusNotifier-compatible tray running (e.g., Waybar)
- For Waybar, enable the tray module in your config:
  ```json
  "modules-right": ["tray", ...],
  "tray": {
    "spacing": 10
  }
  ```

### Stale daemon/PID file

**Symptom**: "Found running daemon" but nothing happens

**Solution**: The tool automatically detects and cleans up stale PID files. Just run the command again.

### Window class changes

Some applications use different classes for different windows. Find the exact class:

```bash
hyprctl clients | grep -A 5 "title: YourApp"
```

---

## Requirements

- **Hyprland** — The compositor (obviously)
- **System tray** — StatusNotifier-compatible (Waybar, etc.)
- **Rust** — For building from source

---

## License

MIT License - Do whatever you want with this code.
