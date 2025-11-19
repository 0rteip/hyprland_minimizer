//! Lock file management for preventing multiple daemon instances.
//! 
//! This module handles exclusive locking per application to ensure only one
//! daemon process runs for each managed application. It also provides
//! inter-process communication through signals.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

/// Returns the path to the lock file for a given application.
fn get_lock_file_path(app_name: &str) -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime_dir).join(format!("hyprland-minimizer-{}.pid", app_name))
}

/// Acquires an exclusive lock for the application.
/// 
/// If another instance is running, sends it a SIGUSR1 signal to toggle the window
/// and returns the PID of the existing instance. Otherwise, creates a lock file
/// with the current PID.
/// 
/// # Returns
/// - `Ok(Some(pid))` if another instance is running (pid of existing instance)
/// - `Ok(None)` if lock was acquired successfully
/// - `Err(_)` if lock file operations failed
pub fn acquire_lock(app_name: &str) -> Result<Option<i32>> {
    let lock_file = get_lock_file_path(app_name);
    
    // Check if a previous instance exists
    if lock_file.exists() {
        if let Ok(old_pid_str) = fs::read_to_string(&lock_file) {
            if let Ok(old_pid) = old_pid_str.trim().parse::<i32>() {
                // Check if the process is actually running
                let check_result = Command::new("kill")
                    .arg("-0")  // Signal 0 just checks if process exists
                    .arg(old_pid.to_string())
                    .status();
                
                if check_result.is_ok() && check_result.unwrap().success() {
                    println!("[Lock] Found running daemon with PID {}. Sending toggle signal...", old_pid);
                    // Send SIGUSR1 signal to toggle the window
                    let _ = Command::new("kill")
                        .arg("-USR1")
                        .arg(old_pid.to_string())
                        .status();
                    return Ok(Some(old_pid));
                } else {
                    println!("[Lock] Stale PID file found (process {} not running). Cleaning up...", old_pid);
                    let _ = fs::remove_file(&lock_file);
                }
            }
        }
    }
    
    // Write our PID to the lock file
    let current_pid = std::process::id();
    let mut file = fs::File::create(&lock_file)
        .with_context(|| format!("Failed to create lock file: {:?}", lock_file))?;
    write!(file, "{}", current_pid)
        .with_context(|| "Failed to write PID to lock file")?;
    
    println!("[Lock] Acquired lock with PID {} - Starting daemon mode", current_pid);
    Ok(None)
}

/// Releases the lock file when the application exits.
/// 
/// Only removes the lock file if it contains the current process's PID,
/// preventing removal of lock files from other processes.
pub fn release_lock(app_name: &str) {
    let lock_file = get_lock_file_path(app_name);
    if lock_file.exists() {
        if let Ok(pid_str) = fs::read_to_string(&lock_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                // Only remove if it's our PID
                if pid == std::process::id() {
                    let _ = fs::remove_file(&lock_file);
                    println!("[Lock] Released lock");
                }
            }
        }
    }
}
