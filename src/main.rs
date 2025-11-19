//! Hyprland Minimizer - A minimize-to-tray utility for Hyprland.
//! 
//! This application creates system tray icons for Hyprland windows and allows
//! toggling them between workspaces and a special "minimized" workspace.

mod config;
mod dbus;
mod hyprland;
mod launcher;
mod lock;

use anyhow::{Context, Result};
use clap::Parser;
use std::sync::Arc;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Notify;
use tokio::time::{interval, Duration};
use tokio_stream::StreamExt;
use zbus::ConnectionBuilder;

use config::Config;
use dbus::{DbusMenu, StatusNotifierItem, DBUS_WATCHER_NAME, REREGISTER_DELAY_MS};
use hyprland::WindowInfo;

/// Interval for checking if the managed window still exists.
const WINDOW_CHECK_INTERVAL_SECS: u64 = 2;

/// Command-line arguments parser.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The workspace/app identifier (e.g., whatsapp, spotify)
    app_name: Option<String>,
}

// --- Main Application Logic ---

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();

    // 1. Load configuration
    let config = Config::load()?;

    // 2. Validate app name parameter
    let app_name = match args.app_name {
        Some(name) if config.apps.contains_key(&name) => name,
        Some(name) => {
            eprintln!("Error: Unknown app '{}'", name);
            eprintln!("Available apps: {}", config.apps.keys().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
            eprintln!("\nEdit the config file at: {:?}", Config::get_config_path());
            std::process::exit(1);
        }
        _none => {
            println!("Usage: {} <app_name>", std::env::args().next().unwrap_or_else(|| "hyprland-minimizer".to_string()));
            println!("Available apps: {}", config.apps.keys().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
            println!("\nEdit the config file at: {:?}", Config::get_config_path());
            std::process::exit(1);
        }
    };

    let app_config = config.apps.get(&app_name).unwrap().clone();

    // 3. Check if daemon is already running
    if let Some(existing_pid) = lock::acquire_lock(&app_name)? {
        println!("Daemon already running with PID {}. Signal sent.", existing_pid);
        std::process::exit(0);
    }

    // 4. Find or launch the application
    let clients: Vec<WindowInfo> = hyprland::hyprctl("clients")
        .context("Failed to get client list from Hyprland.")?;
    let (mut window_info, is_newly_launched) = match clients.into_iter().find(|c| c.class == app_config.class) {
        Some(window) => (window, false),
        None => {
            launcher::launch_application(&app_config)?;
            
            // Wait for the application to appear with retry mechanism
            let timeout_secs = app_config.launch_timeout.unwrap_or(10);
            let max_attempts = (timeout_secs * 2).max(10) as usize; // Check every ~500ms
            let mut found_window = None;
            
            println!("[Launch] Waiting up to {} seconds for '{}' to appear...", timeout_secs, app_config.class);
            
            for attempt in 1..=max_attempts {
                tokio::time::sleep(Duration::from_millis(500)).await;
                
                if let Ok(clients) = hyprland::hyprctl::<Vec<WindowInfo>>("clients") {
                    if let Some(window) = clients.into_iter().find(|c| c.class == app_config.class) {
                        println!("[Launch] Found window after {:.1}s (attempt {})", attempt as f64 * 0.5, attempt);
                        found_window = Some(window);
                        break;
                    }
                }
                
                // Show progress for slow launches
                if attempt % 4 == 0 {
                    println!("[Launch] Still waiting... ({}s elapsed)", attempt / 2);
                }
            }
            
            match found_window {
                Some(w) => (w, true),
                None => {
                    eprintln!("[Error] Failed to find window with class '{}' after {} seconds", 
                              app_config.class, timeout_secs);
                    eprintln!("[Error] The application may have failed to launch or uses a different window class.");
                    eprintln!("[Error] Try running: hyprctl clients | grep -i {}", app_config.name);
                    lock::release_lock(&app_name);
                    std::process::exit(1);
                }
            }
        }
    };

    println!(
        "[Daemon] Managing window: '{}' ({}) on workspace {}",
        window_info.title, window_info.class, window_info.workspace.id
    );

    if window_info.class.is_empty() {
        window_info.class = app_config.class.clone();
    }

    // Wrap in Arc for sharing without cloning the struct
    let window_info = Arc::new(window_info);

    // 5. Perform initial toggle if needed
    if !is_newly_launched {
        // App already exists, toggle it
        let _ = hyprland::handle_window_toggle(&app_config.class).await;
    } else {
        // App just launched
        if app_config.launch_in_background.unwrap_or(false) {
            // Move to special workspace immediately
            println!("[Daemon] Newly launched - moving to special workspace (background)");
            tokio::time::sleep(Duration::from_millis(500)).await; // Give app time to settle
            let _ = hyprland::dispatch(&format!("focuswindow address:{}", window_info.address));
            let _ = hyprland::dispatch(&format!(
                "movetoworkspacesilent special:{},address:{}",
                app_config.class, window_info.address
            ));
        } else {
            // Keep on current workspace
            println!("[Daemon] Newly launched - keeping window on current workspace");
        }
    }

    // 5. Set up the D-Bus services (always create tray icon)
    let exit_notify = Arc::new(Notify::new());

    let notifier_item = StatusNotifierItem {
        window_info: Arc::clone(&window_info),
        exit_notify: Arc::clone(&exit_notify),
    };

    let dbus_menu = DbusMenu {
        window_info: Arc::clone(&window_info),
        exit_notify: Arc::clone(&exit_notify),
    };

    let bus_name = format!(
        "org.kde.StatusNotifierItem.{}.p{}",
        app_name, std::process::id()
    );

    let connection = ConnectionBuilder::session()?
        .name(bus_name.as_str())?
        .serve_at("/StatusNotifierItem", notifier_item)?
        .serve_at("/Menu", dbus_menu)?
        .build()
        .await?;

    // Create an Arc of the connection to share with the watcher task.
    let arc_conn = Arc::new(connection);

    println!("D-Bus service '{}' is running.", bus_name);

    // 6. Initial registration with the StatusNotifierWatcher
    if let Err(e) = dbus::register_with_watcher(&arc_conn, &bus_name).await {
        eprintln!("Could not register with StatusNotifierWatcher: {}", e);
        eprintln!("Is a tray like Waybar running?");
        let _ = hyprland::dispatch(&format!(
            "movetoworkspace {},address:{}",
            window_info.workspace.id, window_info.address
        ));
        anyhow::bail!("Failed to register tray icon.");
    }
    println!("Registration successful.");

    // Task to watch for Waybar restarts and re-register the icon.
    let conn_clone = Arc::clone(&arc_conn);
    let bus_name_clone = bus_name.clone();
    tokio::spawn(async move {
        let dbus_proxy = match zbus::fdo::DBusProxy::new(&*conn_clone).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[Watcher] Failed to connect to D-Bus proxy: {}", e);
                return;
            }
        };

        let mut owner_changes = match dbus_proxy.receive_name_owner_changed().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Watcher] Failed to listen for owner changes: {}", e);
                return;
            }
        };

        println!("[Watcher] Watching for '{}' restarts...", DBUS_WATCHER_NAME);

        while let Some(signal) = owner_changes.next().await {
            if let Ok(args) = signal.args() {
                if args.name() == DBUS_WATCHER_NAME && args.new_owner().is_some() {
                    println!("[Watcher] Tray service detected. Re-registering icon.");
                    tokio::time::sleep(Duration::from_millis(REREGISTER_DELAY_MS)).await;
                    if let Err(e) = dbus::register_with_watcher(&conn_clone, &bus_name_clone).await {
                        eprintln!("[Watcher] Failed to re-register icon: {}", e);
                    }
                }
            }
        }
    });

    // 7. Set up signal handlers
    let app_class = app_config.class.clone();
    let mut sigusr1 = signal(SignalKind::user_defined1())
        .context("Failed to create SIGUSR1 handler")?;
    
    tokio::spawn(async move {
        while sigusr1.recv().await.is_some() {
            println!("[Signal] Received SIGUSR1 - Toggling window");
            if let Err(e) = hyprland::handle_window_toggle(&app_class).await {
                eprintln!("[Signal] Failed to handle toggle: {}", e);
            }
        }
    });

    // 8. Start a background check to see if the window is closed
    let window_address = window_info.address.clone();
    let exit_notify_clone = Arc::clone(&exit_notify);
    tokio::spawn(async move {
        let mut check_interval = interval(Duration::from_secs(WINDOW_CHECK_INTERVAL_SECS));
        loop {
            check_interval.tick().await;
            match hyprland::hyprctl::<Vec<WindowInfo>>("clients") {
                Ok(clients) => {
                    // Exit only if the window is completely closed
                    if !clients.iter().any(|c| c.address == window_address) {
                        println!("Window closed. Exiting.");
                        exit_notify_clone.notify_one();
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error checking window state: {}", e);
                    exit_notify_clone.notify_one();
                    break;
                }
            }
        }
    });

    // 9. Wait for exit signal
    println!("[Daemon] Running. Send SIGUSR1 to toggle, or close the window to exit.");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n[Daemon] Interrupted by Ctrl+C.");
        }
        _ = exit_notify.notified() => {
            println!("[Daemon] Window closed, exiting.");
        }
    }

    // 10. Release the lock before exiting
    lock::release_lock(&app_name);
    
    println!("[Daemon] Exiting.");
    Ok(())
}
