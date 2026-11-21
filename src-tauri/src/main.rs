#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

const WEBVIEW2_DOWNLOAD_URL: &str = "https://developer.microsoft.com/en-us/microsoft-edge/webview2/";

#[cfg(windows)]
fn ensure_webview2_or_prompt() -> bool {
    if tauri::webview_version().is_ok() {
        return true;
    }

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONWARNING, MB_YESNO,
    };

    let title: Vec<u16> = "DropPilot — WebView2 Required\0".encode_utf16().collect();
    let msg: Vec<u16> = concat!(
        "DropPilot requires the Microsoft Edge WebView2 Runtime, which was not detected on this PC.\n\n",
        "Would you like to open Microsoft's official WebView2 download page in your browser?"
    )
    .encode_utf16()
    .chain(std::iter::once(0))
    .collect();

    // Safe Win32 MessageBoxW call with static null-terminated UTF-16 buffers.
    let response = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            msg.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONWARNING,
        )
    };

    if response == IDYES {
        let _ = open::that(WEBVIEW2_DOWNLOAD_URL);
    }
    false
}

#[cfg(not(windows))]
fn ensure_webview2_or_prompt() -> bool {
    true
}

fn main() {
    if !ensure_webview2_or_prompt() {
        return;
    }
    droppilot_lib::run();
}
