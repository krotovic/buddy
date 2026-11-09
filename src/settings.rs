//! User settings, stored as JSON in the platform config folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::geometry::Edge;
use crate::palette::FrameColor;

/// On / Calm / Off, switched from the tray or the global shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Peeks and notifications as designed.
    #[default]
    On,
    /// Office mode: no idle peeks, minimal movement.
    Calm,
    /// Nothing on screen, tray icon only.
    Off,
}

pub const CUBE_SIZE_MIN: u32 = 120;
pub const CUBE_SIZE_MAX: u32 = 480;
pub const IDLE_INTERVAL_MIN_SECS: u64 = 30;
pub const IDLE_INTERVAL_MAX_SECS: u64 = 10 * 60;
pub const DEFAULT_HOOK_PORT: u16 = 47821;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: Mode,
    pub frame_color: FrameColor,
    /// Width of the cube body in logical pixels (120–480).
    pub cube_size: u32,
    /// Seconds between idle peeks. The plan's range is 2–10 minutes; values
    /// down to 30 s are allowed for the monitor dimming test.
    pub idle_interval_secs: u64,
    /// Screen edge the cube peeks from.
    pub edge: Edge,
    /// Global shortcut that toggles Off, in global-hotkey syntax.
    pub toggle_shortcut: String,
    /// Minutes until a Done item clears itself (1–60); 0 means never.
    pub done_timeout_mins: u64,
    /// Port of the local server Claude Code hooks post to (127.0.0.1).
    pub hook_port: u16,
    /// Secret the hooks send, so other programs cannot post fake events.
    /// Generated on first start.
    pub hook_token: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::On,
            frame_color: FrameColor::Coral,
            cube_size: 200,
            idle_interval_secs: 5 * 60,
            edge: Edge::Bottom,
            toggle_shortcut: "CmdOrCtrl+Alt+C".to_string(),
            done_timeout_mins: 10,
            hook_port: DEFAULT_HOOK_PORT,
            hook_token: String::new(),
        }
    }
}

impl Settings {
    /// Clamp values that may have been edited by hand into their valid ranges.
    pub fn sanitized(mut self) -> Self {
        self.cube_size = self.cube_size.clamp(CUBE_SIZE_MIN, CUBE_SIZE_MAX);
        self.idle_interval_secs = self
            .idle_interval_secs
            .clamp(IDLE_INTERVAL_MIN_SECS, IDLE_INTERVAL_MAX_SECS);
        if self.done_timeout_mins != 0 {
            self.done_timeout_mins = self.done_timeout_mins.clamp(1, 60);
        }
        if self.hook_port < 1024 {
            self.hook_port = DEFAULT_HOOK_PORT;
        }
        self
    }

    /// How long a Done item stays pending; None means until you clear it.
    pub fn done_timeout(&self) -> Option<std::time::Duration> {
        (self.done_timeout_mins > 0)
            .then(|| std::time::Duration::from_secs(self.done_timeout_mins * 60))
    }

    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str::<Settings>(json)?.sanitized())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialise")
    }

    /// Load from `path`. A missing file gives the defaults; a broken file is
    /// logged and also gives the defaults, so the app always starts.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(json) => Self::from_json(&json).unwrap_or_else(|e| {
                log::warn!("ignoring unreadable settings {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write then rename, so a crash never leaves half a file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// `settings.json` in the platform config folder, for example
/// `%APPDATA%\krotovic\Buddy\config\settings.json` on Windows and
/// `~/Library/Application Support/com.krotovic.Buddy/settings.json` on macOS.
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("com", "krotovic", "Buddy")
        .map(|dirs| dirs.config_dir().join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_plan() {
        let s = Settings::default();
        assert_eq!(s.mode, Mode::On);
        assert_eq!(s.frame_color, FrameColor::Coral);
        assert_eq!(s.cube_size, 200);
        assert_eq!(s.idle_interval_secs, 300);
    }

    #[test]
    fn round_trip() {
        let s = Settings {
            mode: Mode::Calm,
            frame_color: FrameColor::Mint,
            cube_size: 320,
            idle_interval_secs: 120,
            edge: Edge::Left,
            toggle_shortcut: "Ctrl+Shift+B".into(),
            done_timeout_mins: 0,
            hook_port: 50000,
            hook_token: "abc".into(),
        };
        assert_eq!(Settings::from_json(&s.to_json()).unwrap(), s);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s = Settings::from_json(r#"{"frame_color":"sky"}"#).unwrap();
        assert_eq!(s.frame_color, FrameColor::Sky);
        assert_eq!(s.cube_size, 200);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        assert!(Settings::from_json(r#"{"future_option":true}"#).is_ok());
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let s = Settings::from_json(
            r#"{"cube_size":5000,"idle_interval_secs":1,"done_timeout_mins":500,"hook_port":80}"#,
        )
        .unwrap();
        assert_eq!(s.cube_size, CUBE_SIZE_MAX);
        assert_eq!(s.idle_interval_secs, IDLE_INTERVAL_MIN_SECS);
        assert_eq!(s.done_timeout_mins, 60);
        assert_eq!(s.hook_port, DEFAULT_HOOK_PORT);
        assert_eq!(Settings::default().done_timeout().unwrap().as_secs(), 600);
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("buddy-test-{}", std::process::id()));
        let path = dir.join("nested").join("settings.json");
        let s = Settings {
            cube_size: 240,
            ..Settings::default()
        };
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn broken_file_gives_defaults() {
        let dir = std::env::temp_dir().join(format!("buddy-test-broken-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
