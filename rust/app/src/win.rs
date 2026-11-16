//! Windows: one PGPM at a time (the named mutex the setup also checks before installing over a
//! running copy), and the pointer's position while a file is dragged over the window (Windows
//! sends the window no pointer moves during a drag, so egui's last position would be stale).

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, POINT};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, MB_ICONINFORMATION, MB_OK, MessageBoxW};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Takes PGPM's single-instance mutex (`PGPM_single_instance_mutex`, the same name the Tk app
/// used and the setup's AppMutex looks for). False when another PGPM holds it: that one gets a
/// "PGPM is already running." box, like the Tk app showed.
pub fn single_instance() -> bool {
    let name = wide("PGPM_single_instance_mutex");
    // SAFETY: plain Win32 calls; the handle stays open until the process exits, which is what
    // keeps the mutex owned
    let exists = unsafe {
        CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        GetLastError() == ERROR_ALREADY_EXISTS
    };
    if exists {
        let (text, title) = (wide("PGPM is already running."), wide("PGPM"));
        // SAFETY: both strings are NUL-terminated and outlive the call
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONINFORMATION) };
    }
    !exists
}

/// The pointer in window points (top-left origin), from the system cursor.
pub fn pointer_in_window(ctx: &egui::Context) -> Option<egui::Pos2> {
    let inner = ctx.input(|i| i.viewport().inner_rect)?;
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: `p` is a valid POINT for the call to fill in
    if unsafe { GetCursorPos(&mut p) } == 0 {
        return None;
    }
    let ppp = ctx.pixels_per_point();
    Some(egui::pos2(p.x as f32 / ppp - inner.min.x, p.y as f32 / ppp - inner.min.y))
}
