//! Which app is in front, so the cube can stay still while you are in
//! an app on the quiet list (the Claude app by default).

/// The frontmost app: its bundle id on macOS, its executable name (for
/// example `Claude.exe`) on Windows. None when it cannot be read.
pub fn frontmost_app() -> Option<String> {
    imp::frontmost_app()
}

/// Whether `app` is on the `quiet` list. Case does not matter, since
/// Windows file names are case-insensitive.
pub fn is_quiet(app: Option<&str>, quiet: &[String]) -> bool {
    app.is_some_and(|a| quiet.iter().any(|q| q.eq_ignore_ascii_case(a)))
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::NSWorkspace;

    pub fn frontmost_app() -> Option<String> {
        // Called on the main thread, where AppKit keeps this up to date.
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        Some(app.bundleIdentifier()?.to_string())
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    pub fn frontmost_app() -> Option<String> {
        // SAFETY: plain Win32 calls with valid out pointers; the process
        // handle is closed before returning.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok =
                QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(process);
            if ok == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit(['\\', '/']).next().map(str::to_owned)
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    pub fn frontmost_app() -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_list_matches_ignoring_case() {
        let quiet = vec![
            "com.anthropic.claudefordesktop".to_string(),
            "Claude.exe".to_string(),
        ];
        assert!(is_quiet(Some("claude.exe"), &quiet));
        assert!(is_quiet(Some("com.anthropic.claudefordesktop"), &quiet));
        assert!(!is_quiet(Some("com.googlecode.iterm2"), &quiet));
        assert!(!is_quiet(None, &quiet));
    }
}
