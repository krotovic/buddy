//! The mouse, read globally. The window stays click-through, so clicks
//! on the cube are noticed by polling the cursor and the left button
//! while the cube is out; the click still reaches the app underneath, and
//! no app loses keyboard focus.

/// Cursor position in physical pixels (the same space as winit's window
/// positions) and whether the left button is down. `scale` is the
/// window's scale factor, used on macOS to turn points into pixels.
pub fn global(scale: f64) -> Option<(f64, f64, bool)> {
    imp::global(scale)
}

/// Whether the left button went down since the last poll.
#[derive(Debug, Default)]
pub struct ClickEdge {
    was_down: bool,
}

impl ClickEdge {
    pub fn pressed(&mut self, down: bool) -> bool {
        let pressed = down && !self.was_down;
        self.was_down = down;
        pressed
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen};

    pub fn global(scale: f64) -> Option<(f64, f64, bool)> {
        let mtm = MainThreadMarker::new()?;
        // AppKit counts points from the bottom-left of the primary screen;
        // winit counts from its top-left.
        let screens = NSScreen::screens(mtm);
        let primary = screens.firstObject()?;
        let height = primary.frame().size.height;
        let p = NSEvent::mouseLocation();
        let down = NSEvent::pressedMouseButtons() & 1 != 0;
        Some((p.x * scale, (height - p.y) * scale, down))
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    pub fn global(_scale: f64) -> Option<(f64, f64, bool)> {
        let mut p = POINT { x: 0, y: 0 };
        // SAFETY: valid out pointer. winit makes the process per-monitor
        // DPI aware, so these are physical pixels.
        unsafe {
            if GetCursorPos(&mut p) == 0 {
                return None;
            }
            let down = (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0;
            Some((p.x as f64, p.y as f64, down))
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    pub fn global(_scale: f64) -> Option<(f64, f64, bool)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_button_clicks_once() {
        let mut c = ClickEdge::default();
        assert!(!c.pressed(false));
        assert!(c.pressed(true));
        assert!(!c.pressed(true));
        assert!(!c.pressed(false));
        assert!(c.pressed(true));
    }
}
