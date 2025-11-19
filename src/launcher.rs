//! Application launcher module.
//! 
//! This module handles launching configured applications and sending
//! desktop notifications when applications start.

use crate::config::AppConfig;
use anyhow::{Context, Result};
use std::process::Command;

/// Launches an application based on its configuration.
/// 
/// Optionally sends a desktop notification if `notify_name` is configured.
/// 
/// # Arguments
/// * `app_config` - The application configuration containing launch command and notification settings
/// 
/// # Returns
/// * `Ok(())` if the application was launched successfully
/// * `Err(_)` if the launch command failed or no command was specified
pub fn launch_application(app_config: &AppConfig) -> Result<()> {
    println!("Launching {}...", app_config.name);
    
    // Send notification if notify_name is specified
    if let Some(notify_name) = &app_config.notify_name {
        let icon = app_config.icon.as_deref().unwrap_or(&app_config.class);
        let _ = Command::new("notify-send")
            .args(&["-a", notify_name, "Launched", "-i", icon, "-r", "2590", "-u", "low"])
            .spawn();
    }

    if app_config.command.is_empty() {
        anyhow::bail!("No command specified for {}", app_config.name);
    }

    Command::new(&app_config.command[0])
        .args(&app_config.command[1..])
        .spawn()
        .with_context(|| format!("Failed to launch {}", app_config.name))?;

    Ok(())
}
