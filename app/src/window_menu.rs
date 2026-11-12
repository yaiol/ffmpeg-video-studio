//! The system's own window menu (Restore, Move, Size, Minimize, Maximize, Close), opened at the pointer.
//!
//! The window has no system frame, so nothing opens that menu by itself any more: the app's window bar reports a
//! right-click and this shows the menu the system keeps for the window. It is the real one - the system's words,
//! in the system's language - not a copy. Windows only: the other systems have no such menu, and there the call
//! does nothing.

/// Show the window menu at the pointer and carry out what is picked.
#[cfg(windows)]
pub fn show() {
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnableMenuItem, GetCursorPos, GetSystemMenu, PostMessageW, TrackPopupMenu, MF_BYCOMMAND, MF_ENABLED, MF_GRAYED, SC_MAXIMIZE, SC_MOVE,
        SC_RESTORE, SC_SIZE, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_SYSCOMMAND,
    };

    let window = dioxus::desktop::window();
    let hwnd = window.window.hwnd() as HWND;
    let maximized = window.is_maximized();
    // SAFETY: plain calls on this window's own handle, made on the thread that owns the window.
    unsafe {
        let menu = GetSystemMenu(hwnd, 0);
        if menu.is_null() {
            return;
        }
        // The system greys these itself when IT opens the menu; opened by hand, they are set here.
        let (when_normal, when_maximized) = if maximized { (MF_GRAYED, MF_ENABLED) } else { (MF_ENABLED, MF_GRAYED) };
        EnableMenuItem(menu, SC_RESTORE, MF_BYCOMMAND | when_maximized);
        EnableMenuItem(menu, SC_MOVE, MF_BYCOMMAND | when_normal);
        EnableMenuItem(menu, SC_SIZE, MF_BYCOMMAND | when_normal);
        EnableMenuItem(menu, SC_MAXIMIZE, MF_BYCOMMAND | when_normal);
        let mut at = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut at) == 0 {
            return;
        }
        // The menu answers with the command picked (0 = none); the window then gets it as if the system had sent it.
        let picked = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, at.x, at.y, 0, hwnd, std::ptr::null());
        if picked != 0 {
            PostMessageW(hwnd, WM_SYSCOMMAND, picked as usize, 0);
        }
    }
}

#[cfg(not(windows))]
pub fn show() {}
