//! Tray (menu bar) icon, its menu, and the global shortcut.
//!
//! Only built on Windows and macOS. Other platforms get a stub so the core
//! can be developed and tested anywhere.

use crate::geometry::Edge;
use crate::palette::FrameColor;
use crate::settings::{Mode, Settings};

/// Something the user picked in the tray menu or pressed on the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetMode(Mode),
    ToggleOff,
    PeekNow,
    SetInterval(u64),
    SetSize(u32),
    SetEdge(Edge),
    SetColor(FrameColor),
    OpenSettingsFile,
    Quit,
}

/// Idle interval presets in seconds. 30 s is for the dimming test.
pub const INTERVALS: [(u64, &str); 4] = [
    (30, "30 seconds (test)"),
    (120, "2 minutes"),
    (300, "5 minutes"),
    (600, "10 minutes"),
];

/// Cube size presets (logical px).
pub const SIZES: [(u32, &str); 4] = [(140, "S"), (200, "M"), (300, "L"), (420, "XL")];

/// Menu item ids are plain strings so they survive the trip through the
/// event loop; this turns one back into a command.
pub fn parse_menu_id(id: &str) -> Option<Command> {
    let (kind, value) = id.split_once(':').unwrap_or((id, ""));
    Some(match kind {
        "mode" => {
            Command::SetMode(serde_json::from_value(serde_json::Value::String(value.into())).ok()?)
        }
        "peek" => Command::PeekNow,
        "interval" => Command::SetInterval(value.parse().ok()?),
        "size" => Command::SetSize(value.parse().ok()?),
        "edge" => {
            Command::SetEdge(serde_json::from_value(serde_json::Value::String(value.into())).ok()?)
        }
        "color" => {
            Command::SetColor(serde_json::from_value(serde_json::Value::String(value.into())).ok()?)
        }
        "open-settings" => Command::OpenSettingsFile,
        "quit" => Command::Quit,
        _ => return None,
    })
}

fn enum_id<T: serde::Serialize>(kind: &str, value: T) -> String {
    let v = serde_json::to_value(value).expect("serialise");
    format!("{kind}:{}", v.as_str().unwrap_or_default())
}

pub fn mode_id(m: Mode) -> String {
    enum_id("mode", m)
}
pub fn edge_id(e: Edge) -> String {
    enum_id("edge", e)
}
pub fn color_id(c: FrameColor) -> String {
    enum_id("color", c)
}

/// The ids that should show a check mark for these settings.
pub fn checked_ids(s: &Settings) -> Vec<String> {
    vec![
        mode_id(s.mode),
        format!("interval:{}", s.idle_interval_secs),
        format!("size:{}", s.cube_size),
        edge_id(s.edge),
        color_id(s.frame_color),
    ]
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use real::Tray;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub use stub::Tray;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod real {
    use super::*;
    use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};
    use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    pub struct Tray {
        icon: TrayIcon,
        checks: Vec<CheckMenuItem>,
        _hotkeys: Option<(GlobalHotKeyManager, HotKey)>,
        hotkey_id: Option<u32>,
    }

    fn icon(color: FrameColor) -> anyhow::Result<Icon> {
        let size = 64;
        Ok(Icon::from_rgba(
            crate::cube::icon_rgba(color, size)?,
            size,
            size,
        )?)
    }

    impl Tray {
        /// Must be called on the main thread once the event loop runs.
        pub fn new(settings: &Settings) -> anyhow::Result<Self> {
            let menu = Menu::new();
            let mut checks = Vec::new();
            let mut check = |id: String, label: &str| {
                let item = CheckMenuItem::with_id(id, label, true, false, None);
                checks.push(item.clone());
                item
            };

            menu.append(&check(mode_id(Mode::On), "On"))?;
            menu.append(&check(mode_id(Mode::Calm), "Calm"))?;
            menu.append(&check(mode_id(Mode::Off), "Off"))?;
            menu.append(&PredefinedMenuItem::separator())?;
            menu.append(&MenuItem::with_id("peek", "Peek now", true, None))?;

            let interval = Submenu::new("Peek every", true);
            for (secs, label) in INTERVALS {
                interval.append(&check(format!("interval:{secs}"), label))?;
            }
            menu.append(&interval)?;

            let size = Submenu::new("Size", true);
            for (px, label) in SIZES {
                size.append(&check(format!("size:{px}"), &format!("{label} ({px} px)")))?;
            }
            menu.append(&size)?;

            let edge = Submenu::new("Edge", true);
            for e in Edge::ALL {
                edge.append(&check(edge_id(e), e.label()))?;
            }
            menu.append(&edge)?;

            let colour = Submenu::new("Frame colour", true);
            for c in FrameColor::ALL {
                colour.append(&check(color_id(c), c.label()))?;
            }
            menu.append(&colour)?;

            menu.append(&PredefinedMenuItem::separator())?;
            menu.append(&MenuItem::with_id(
                "open-settings",
                "Open settings file",
                true,
                None,
            ))?;
            menu.append(&MenuItem::with_id("quit", "Quit Buddy", true, None))?;

            let icon = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Buddy")
                .with_icon(icon(settings.frame_color)?)
                .build()?;

            let hotkeys = match settings.toggle_shortcut.parse::<HotKey>() {
                Ok(hotkey) => GlobalHotKeyManager::new()
                    .and_then(|m| m.register(hotkey).map(|_| (m, hotkey)))
                    .map_err(|e| {
                        log::warn!("shortcut {} not registered: {e}", settings.toggle_shortcut)
                    })
                    .ok(),
                Err(e) => {
                    log::warn!("bad shortcut {:?}: {e}", settings.toggle_shortcut);
                    None
                }
            };
            let hotkey_id = hotkeys.as_ref().map(|(_, h)| h.id());

            let mut tray = Self {
                icon,
                checks,
                _hotkeys: hotkeys,
                hotkey_id,
            };
            tray.sync(settings);
            Ok(tray)
        }

        pub fn sync(&mut self, settings: &Settings) {
            let on = checked_ids(settings);
            for item in &self.checks {
                item.set_checked(on.iter().any(|id| id == &item.id().0));
            }
        }

        pub fn set_color(&mut self, color: FrameColor) {
            match icon(color) {
                Ok(i) => {
                    if let Err(e) = self.icon.set_icon(Some(i)) {
                        log::warn!("tray icon: {e}");
                    }
                }
                Err(e) => log::warn!("tray icon: {e}"),
            }
        }

        pub fn is_toggle_hotkey(&self, id: u32) -> bool {
            self.hotkey_id == Some(id)
        }
    }

    /// Forward tray menu and shortcut events into the winit event loop.
    pub fn forward_events<T: Send + 'static>(
        proxy: winit::event_loop::EventLoopProxy<T>,
        on_menu: fn(String) -> T,
        on_hotkey: fn(u32) -> T,
    ) {
        let proxy = std::sync::Mutex::new(proxy);
        let proxy = std::sync::Arc::new(proxy);
        let p = proxy.clone();
        tray_icon::menu::MenuEvent::set_event_handler(Some(
            move |e: tray_icon::menu::MenuEvent| {
                let _ = p.lock().unwrap().send_event(on_menu(e.id.0));
            },
        ));
        global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(
            move |e: global_hotkey::GlobalHotKeyEvent| {
                if e.state == global_hotkey::HotKeyState::Pressed {
                    let _ = proxy.lock().unwrap().send_event(on_hotkey(e.id));
                }
            },
        ));
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use real::forward_events;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod stub {
    use super::*;

    /// No tray on this platform; the app still runs for development.
    pub struct Tray;

    impl Tray {
        pub fn new(_: &Settings) -> anyhow::Result<Self> {
            log::warn!("no tray icon on this platform");
            Ok(Tray)
        }
        pub fn sync(&mut self, _: &Settings) {}
        pub fn set_color(&mut self, _: FrameColor) {}
        pub fn is_toggle_hotkey(&self, _: u32) -> bool {
            false
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn forward_events<T: Send + 'static>(
    _: winit::event_loop::EventLoopProxy<T>,
    _: fn(String) -> T,
    _: fn(u32) -> T,
) {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_round_trip() {
        assert_eq!(
            parse_menu_id(&mode_id(Mode::Calm)),
            Some(Command::SetMode(Mode::Calm))
        );
        assert_eq!(
            parse_menu_id(&edge_id(Edge::Left)),
            Some(Command::SetEdge(Edge::Left))
        );
        assert_eq!(
            parse_menu_id(&color_id(FrameColor::Lavender)),
            Some(Command::SetColor(FrameColor::Lavender))
        );
        assert_eq!(
            parse_menu_id("interval:120"),
            Some(Command::SetInterval(120))
        );
        assert_eq!(parse_menu_id("size:300"), Some(Command::SetSize(300)));
        assert_eq!(parse_menu_id("peek"), Some(Command::PeekNow));
        assert_eq!(parse_menu_id("quit"), Some(Command::Quit));
        assert_eq!(parse_menu_id("nonsense:1"), None);
        assert_eq!(parse_menu_id("size:big"), None);
    }

    #[test]
    fn default_settings_have_a_checked_item_in_every_group() {
        let s = Settings::default();
        let ids = checked_ids(&s);
        assert!(ids.contains(&"mode:on".to_string()));
        assert!(
            INTERVALS
                .iter()
                .any(|(v, _)| ids.contains(&format!("interval:{v}")))
        );
        assert!(
            SIZES
                .iter()
                .any(|(v, _)| ids.contains(&format!("size:{v}")))
        );
        assert!(ids.contains(&"edge:bottom".to_string()));
        assert!(ids.contains(&"color:coral".to_string()));
    }
}
