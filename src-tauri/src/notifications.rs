use tauri::AppHandle;

#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::OnceLock;
#[cfg(windows)]
use tracing::{info, warn};

#[cfg(not(windows))]
use tauri_plugin_notification::NotificationExt;

pub const APP_USER_MODEL_ID: &str = "io.dropilot.app";
pub const APP_DISPLAY_NAME: &str = "Dropilot";

#[cfg(windows)]
const EMBEDDED_TOAST_ICON_PNG: &[u8] = include_bytes!("../icons/128x128.png");

#[cfg(windows)]
static TOAST_ICON_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

#[cfg(windows)]
fn to_wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn ensure_toast_icon_file() -> Option<PathBuf> {
    TOAST_ICON_PATH
        .get_or_init(|| {
            let dir = dirs::data_local_dir()?.join("Dropilot");
            if let Err(err) = fs::create_dir_all(&dir) {
                warn!(error = %err, "Failed creating Dropilot local data dir for toast icon");
                return None;
            }
            let icon_path = dir.join("dropilot-toast-icon.png");
            let needs_write = match fs::metadata(&icon_path) {
                Ok(meta) => meta.len() != EMBEDDED_TOAST_ICON_PNG.len() as u64,
                Err(_) => true,
            };
            if needs_write {
                if let Err(err) = fs::write(&icon_path, EMBEDDED_TOAST_ICON_PNG) {
                    warn!(error = %err, "Failed writing Dropilot toast icon PNG");
                    return None;
                }
            }
            Some(icon_path)
        })
        .clone()
}

/// Registers Dropilot's per-user Windows AppUserModelID (`io.dropilot.app`) with its own
/// display name and icon so Windows notifications always display "Dropilot" instead of
/// falling back to "Windows PowerShell". Requires zero elevation (HKCU only).
#[cfg(windows)]
pub fn register_windows_toast_identity() {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let aumid_wide = to_wide_null(APP_USER_MODEL_ID);
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(aumid_wide.as_ptr());
    }

    let icon_path = ensure_toast_icon_file();
    let subkey = format!("Software\\Classes\\AppUserModelId\\{APP_USER_MODEL_ID}");
    let subkey_wide = to_wide_null(&subkey);

    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey_wide.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        );
        if status != ERROR_SUCCESS {
            warn!(status, "Could not register HKCU AppUserModelId for notifications");
            return;
        }

        let set_sz = |name: &str, val: &str| {
            let name_w = to_wide_null(name);
            let val_w = to_wide_null(val);
            let byte_len = (val_w.len() * std::mem::size_of::<u16>()) as u32;
            let _ = RegSetValueExW(
                hkey,
                name_w.as_ptr(),
                0,
                REG_SZ,
                val_w.as_ptr() as *const u8,
                byte_len,
            );
        };

        set_sz("DisplayName", APP_DISPLAY_NAME);
        set_sz("IconBackgroundColor", "0");
        if let Some(ref path) = icon_path {
            if let Some(path_str) = path.to_str() {
                set_sz("IconUri", path_str);
            }
        }

        let _ = RegCloseKey(hkey);
    }

    info!(aumid = APP_USER_MODEL_ID, "Registered Dropilot Windows notification identity");
}

#[cfg(not(windows))]
pub fn register_windows_toast_identity() {}

/// Sends a native desktop toast attributed cleanly to Dropilot (never PowerShell).
pub fn send_desktop_toast(_app: &AppHandle, title: &str, body: &str) {
    #[cfg(windows)]
    {
        let mut toast = tauri_winrt_notification::Toast::new(APP_USER_MODEL_ID)
            .title(title)
            .text1(body);
        if let Some(icon_path) = ensure_toast_icon_file() {
            toast = toast.icon(
                &icon_path,
                tauri_winrt_notification::IconCrop::Circular,
                APP_DISPLAY_NAME,
            );
        }
        if let Err(err) = toast.show() {
            warn!(error = %err, "Failed displaying Windows toast notification");
        }
    }

    #[cfg(not(windows))]
    {
        let _ = _app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    }
}
