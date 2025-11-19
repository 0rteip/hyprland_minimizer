//! Configuration management module for hyprland-minimizer.
//! 
//! This module handles loading and validating the application configuration
//! from TOML files. It manages application-specific settings including
//! window classes, icons, launch commands, and behavior options.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Configuration for a single managed application.
#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    /// Display name of the application
    pub name: String,
    /// Hyprland window class to track
    pub class: String,
    /// Icon name for tray icon (optional, defaults to class)
    pub icon: Option<String>,
    /// Command and arguments to launch the application
    pub command: Vec<String>,
    /// Name to use for desktop notifications (optional)
    pub notify_name: Option<String>,
    /// Whether to launch app directly in hidden special workspace
    pub launch_in_background: Option<bool>,
    /// Maximum time to wait for application launch in seconds (default: 10)
    pub launch_timeout: Option<u64>,
}

/// Root configuration structure containing all managed apps.
#[derive(Deserialize, Debug)]
pub struct Config {
    /// Map of app identifiers to their configurations
    pub apps: HashMap<String, AppConfig>,
}

impl Config {
    /// Loads configuration from the standard config file location.
    /// Creates a default config file if it doesn't exist.
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_path();
        
        if !config_path.exists() {
            Self::create_default_config(&config_path)?;
            println!("[Config] Created default config at: {:?}", config_path);
        }
        
        let config_str = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
        
        toml::from_str(&config_str)
            .with_context(|| "Failed to parse config file")
    }
    
    /// Returns the path to the configuration file.
    /// Uses XDG_CONFIG_HOME if set, otherwise falls back to ~/.config
    pub fn get_config_path() -> PathBuf {
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                    .join(".config")
            });
        config_dir.join("hyprland-minimizer").join("config.toml")
    }
    
    /// Creates a default configuration file by copying the example config.
    fn create_default_config(path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }
        
        // Try to copy the example config file from common locations
        let example_paths = [
            "/usr/share/hyprland-minimizer/config.example.toml",
            "/usr/local/share/hyprland-minimizer/config.example.toml",
            concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.toml"),
        ];
        
        for example_path in &example_paths {
            if PathBuf::from(example_path).exists() {
                fs::copy(example_path, path)
                    .with_context(|| format!("Failed to copy example config from: {}", example_path))?;
                return Ok(());
            }
        }
        
        // Fallback: create a minimal config if example file is not found
        let minimal_config = r#"# Hyprland Minimizer Configuration
# Add your applications here
# See: https://github.com/Simon-Martens/hyprland-minimizer for examples

[apps.example]
name = "Example App"
class = "example-class"
icon = "application-default-icon"
command = ["example-command"]
launch_in_background = false
launch_timeout = 10

"#;
        
        fs::write(path, minimal_config)
            .with_context(|| format!("Failed to write default config to: {:?}", path))?;
        
        eprintln!("[Warning] Example config file not found. Created minimal config.");
        eprintln!("[Warning] Please edit {:?} to add your applications.", path);
        
        Ok(())
    }
}
