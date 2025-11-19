//! Hyprland window manager interaction module.
//! 
//! This module provides functions and data structures for interacting with
//! the Hyprland compositor through the hyprctl command-line utility.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;

/// Represents a Hyprland workspace.
#[derive(Deserialize, Debug, Clone)]
pub struct Workspace {
    pub id: i32,
}

/// Information about a window in Hyprland.
#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct WindowInfo {
    /// Unique address of the window
    pub address: String,
    /// The workspace this window belongs to
    pub workspace: Workspace,
    /// Window title
    pub title: String,
    /// Window class (used for matching)
    pub class: String,
}

/// Executes a hyprctl command and returns the parsed JSON output.
pub fn hyprctl<T: for<'de> Deserialize<'de>>(command: &str) -> Result<T> {
    let output = Command::new("hyprctl")
        .arg("-j")
        .arg(command)
        .output()
        .with_context(|| format!("Failed to execute hyprctl command: {}", command))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("hyprctl command '{}' failed: {}", command, stderr);
    }

    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("Failed to parse JSON from hyprctl command: {}", command))
}

/// Executes a hyprctl dispatch command.
pub fn dispatch(command: &str) -> Result<()> {
    let status = Command::new("hyprctl")
        .arg("dispatch")
        .arg(command)
        .status()
        .with_context(|| format!("Failed to execute hyprctl dispatch: {}", command))?;

    if !status.success() {
        anyhow::bail!("hyprctl dispatch command '{}' failed", command);
    }
    Ok(())
}

/// Toggles a special workspace and brings it to the front.
pub fn toggle_special_workspace(class: &str) -> Result<()> {
    dispatch(&format!("togglespecialworkspace {}", class))?;
    dispatch("centerwindow")?;
    dispatch("movetoworkspace +0")?;
    dispatch("alterzorder top")
}

/// Handles window toggling between workspaces based on current state.
/// 
/// This function implements the core window management logic:
/// - If in special workspace: move to active workspace
/// - If in current workspace: move to special workspace
/// - If in different workspace: move to current workspace
pub async fn handle_window_toggle(workspace_name: &str) -> Result<()> {
    let clients: Vec<WindowInfo> = hyprctl("clients")
        .context("Failed to get client list")?;
    
    let window = match clients.iter().find(|c| c.class == workspace_name) {
        Some(w) => w,
        None => {
            println!("[Toggle] Window not found, ignoring signal");
            return Ok(());
        }
    };
    
    let current_workspace = hyprctl::<Workspace>("activeworkspace")?;
    
    if window.workspace.id < 0 {
        // Window is in special workspace, move to active workspace
        println!("[Toggle] Moving from special workspace to active");
        toggle_special_workspace(workspace_name)?;
    } else if window.workspace.id == current_workspace.id {
        // Window is in current workspace, move to special workspace
        println!("[Toggle] Moving from current workspace to special");
        dispatch(&format!("focuswindow initialclass:{}", workspace_name))?;
        dispatch(&format!(
            "movetoworkspacesilent special:{},address:{}",
            workspace_name, window.address
        ))?;
    } else {
        // Window is in different workspace, move to current
        println!("[Toggle] Moving from workspace {} to current", window.workspace.id);
        dispatch(&format!("movetoworkspace +0,address:{}", window.address))?;
        dispatch("centerwindow")?;
        dispatch("alterzorder top")?;
    }
    
    Ok(())
}
