//! D-Bus interface implementations for system tray integration.
//! 
//! This module implements the StatusNotifierItem protocol (used by Waybar and
//! other system trays) and the DBusMenu protocol for context menus.

use crate::hyprland::{self, WindowInfo};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Notify;
use zbus::zvariant::{ObjectPath, Value};
use zbus::dbus_interface;

/// D-Bus service name for the StatusNotifierWatcher.
pub const DBUS_WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";

/// D-Bus object path for the StatusNotifierWatcher.
pub const DBUS_WATCHER_PATH: &str = "/StatusNotifierWatcher";

/// Delay before re-registering with the watcher after it restarts.
pub const REREGISTER_DELAY_MS: u64 = 100;

/// Registers the status notifier item with the StatusNotifierWatcher.
pub async fn register_with_watcher(conn: &zbus::Connection, bus_name: &str) -> anyhow::Result<()> {
    let watcher_proxy: zbus::Proxy<'_> = zbus::ProxyBuilder::new_bare(conn)
        .interface(DBUS_WATCHER_NAME)?
        .path(DBUS_WATCHER_PATH)?
        .destination(DBUS_WATCHER_NAME)?
        .build()
        .await?;
    watcher_proxy
        .call_method("RegisterStatusNotifierItem", &(bus_name,))
        .await?;
    Ok(())
}

/// Implementation of the DBusMenu interface for the context menu.
pub struct DbusMenu {
    pub window_info: Arc<WindowInfo>,
    pub exit_notify: Arc<Notify>,
}

#[dbus_interface(name = "com.canonical.dbusmenu")]
impl DbusMenu {
    /// Returns the menu layout structure.
    fn get_layout(
        &self,
        _parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> (u32, (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>)) {
        println!("[D-Bus Menu] GetLayout called.");

        let create_menu_item = |id: i32, label: String| -> Value {
            let mut props = HashMap::new();
            props.insert("type".to_string(), Value::from("standard"));
            props.insert("label".to_string(), Value::from(label));
            Value::from((id, props, Vec::<Value>::new()))
        };

        let items = vec![
            create_menu_item(1, format!("Toggle {}", self.window_info.title)),
            create_menu_item(
                2,
                format!("Restore to workspace ({})", self.window_info.workspace.id),
            ),
            create_menu_item(3, format!("Close {}", self.window_info.title)),
        ];

        let mut root_props = HashMap::new();
        root_props.insert("children-display".to_string(), Value::from("submenu"));

        let root_layout = (0i32, root_props, items);
        let revision = 2u32;
        println!("[D-Bus Menu] Serving layout revision {}: {:?}", revision, root_layout);
        (revision, root_layout)
    }

    /// Returns properties for a group of menu items.
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, Value<'_>>)> {
        println!("[D-Bus Menu] GetGroupProperties called for IDs: {:?}", ids);
        let mut result = Vec::new();
        for id in ids {
            let mut props = HashMap::new();
            let label = match id {
                1 => format!("Toggle {}", self.window_info.title),
                2 => format!("Restore to workspace ({})", self.window_info.workspace.id),
                3 => format!("Close {}", self.window_info.title),
                _ => continue,
            };
            props.insert("label".to_string(), Value::from(label));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
            props.insert("type".to_string(), Value::from("standard"));
            result.push((id, props));
        }
        println!("[D-Bus Menu] Returning properties: {:?}", result);
        result
    }

    /// Handles a batch of click events (used by Waybar).
    fn event_group(&self, events: Vec<(i32, String, Value<'_>, u32)>) {
        println!(
            "[D-Bus Menu] EventGroup received with {} events",
            events.len()
        );
        for (id, event_id, data, timestamp) in events {
            self.event(id, &event_id, data, timestamp);
        }
    }

    /// Handles a single click event on a menu item.
    fn event(&self, id: i32, event_id: &str, _data: Value<'_>, _timestamp: u32) {
        println!("[D-Bus Menu] Event received: id='{}', event_id='{}'", id, event_id);
        if event_id != "clicked" {
            return;
        }

        let res = match id {
            1 => {
                println!("[D-Bus Menu] 'Toggle' action triggered.");
                // Send signal to ourselves to toggle
                let _ = Command::new("kill")
                    .arg("-USR1")
                    .arg(std::process::id().to_string())
                    .status();
                Ok(())
            }
            2 => {
                println!("[D-Bus Menu] 'Restore to workspace' action triggered.");
                hyprland::dispatch(&format!(
                    "movetoworkspace {},address:{}",
                    self.window_info.workspace.id, self.window_info.address
                ))
                .and_then(|_| {
                    hyprland::dispatch(&format!("focuswindow address:{}", self.window_info.address))
                })
            }
            3 => {
                println!("[D-Bus Menu] 'Close' action triggered.");
                let result = hyprland::dispatch(&format!("closewindow address:{}", self.window_info.address));
                // Exit only when closing the window
                self.exit_notify.notify_one();
                result
            }
            _ => {
                println!("[D-Bus Menu] Clicked on unknown item id: {}", id);
                return;
            }
        };

        if let Err(e) = res {
            eprintln!("[Error] Failed to execute hyprctl dispatch from menu: {}", e);
        }
    }

    /// Handles a batch of "about to show" requests.
    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        println!("[D-Bus Menu] AboutToShowGroup received for IDs: {:?}", ids);
        (vec![], vec![])
    }

    /// Compatibility method for older implementations.
    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    #[dbus_interface(property)]
    fn version(&self) -> u32 {
        3
    }

    #[dbus_interface(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[dbus_interface(property)]
    fn status(&self) -> &str {
        "normal"
    }
}

/// Implementation of the StatusNotifierItem protocol (system tray icon).
pub struct StatusNotifierItem {
    pub window_info: Arc<WindowInfo>,
    pub exit_notify: Arc<Notify>,
}

#[dbus_interface(name = "org.kde.StatusNotifierItem")]
impl StatusNotifierItem {
    // --- Properties ---
    #[dbus_interface(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }

    #[dbus_interface(property)]
    fn id(&self) -> &str {
        &self.window_info.class
    }

    #[dbus_interface(property)]
    fn title(&self) -> &str {
        &self.window_info.title
    }

    #[dbus_interface(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[dbus_interface(property)]
    fn icon_name(&self) -> &str {
        &self.window_info.class
    }

    #[dbus_interface(property)]
    fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        (
            String::new(),
            Vec::new(),
            self.window_info.title.clone(),
            String::new(),
        )
    }

    #[dbus_interface(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[dbus_interface(property)]
    fn menu(&self) -> ObjectPath<'_> {
        ObjectPath::try_from("/Menu").unwrap()
    }

    // --- Methods ---
    
    /// Handles left-click on the tray icon.
    fn activate(&self, _x: i32, _y: i32) {
        println!("[D-Bus] Activate called (left-click) - Sending toggle signal");
        // Send SIGUSR1 to ourselves
        let _ = Command::new("kill")
            .arg("-USR1")
            .arg(std::process::id().to_string())
            .status();
    }

    /// Handles middle-click on the tray icon.
    fn secondary_activate(&self, _x: i32, _y: i32) {
        println!("[D-Bus] SecondaryActivate called (middle-click to close)");
        if let Err(e) =
            hyprland::dispatch(&format!("closewindow address:{}", self.window_info.address))
        {
            eprintln!("[Error] Failed to execute secondary_activate action: {}", e);
        }
        // Exit when closing via middle-click
        self.exit_notify.notify_one();
    }
}
