//! Windows-only window tweaks.

use windows_sys::Win32::UI::WindowsAndMessaging::{LWA_ALPHA, SetLayeredWindowAttributes};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Click-through on Windows adds `WS_EX_LAYERED`. A layered window is not
/// drawn until its layered attributes are set, so set full opacity; the
/// per-pixel alpha still comes from the DirectComposition swapchain.
pub fn keep_layered_window_visible(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    if let RawWindowHandle::Win32(h) = handle.as_raw() {
        let hwnd = h.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        // SAFETY: hwnd is a live window owned by this thread.
        unsafe {
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
        }
    }
}
