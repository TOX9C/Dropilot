# Privacy Policy & Local Storage Map

Dropilot collects **zero telemetry**, **zero analytics**, and **zero crash reports**. Nothing leaves your PC except direct requests to the official Twitch/Kick endpoints (and GitHub's release API if you manually enable or trigger an update check).

## Exact Storage Locations on Windows

Dropilot writes strictly inside per-user AppData folders and Windows Credential Manager:

| Data | Location | Notes |
| --- | --- | --- |
| **Session Tokens** | Windows Credential Manager (`Dropilot/twitch`, `Dropilot/kick`) | Protected by Windows DPAPI; never written to `settings.json`, logs, or frontend state |
| **App Settings** | `%APPDATA%\Dropilot\settings.json` | Written atomically (`.tmp` + sync + rename); contains theme, intervals, campaign toggles, priority list, and window bounds |
| **Rolling Logs** | `%LOCALAPPDATA%\Dropilot\logs\dropilot.log*` | Daily rolling, max 7 files, capped around 5 MB each; all tokens/cookies/auth headers scrubbed via `redact()` |
| **Isolated WebView2 Profile** | `%LOCALAPPDATA%\Dropilot\WebView2\` | Used strictly by Dropilot's own embedded login/inventory/UI windows |
| **Autostart (Optional, Off by Default)** | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Managed by `tauri-plugin-autostart`; never touches HKLM, Task Scheduler, or Windows Services |

## Browser Profile Isolation

Dropilot **never** accesses, reads, or modifies Chrome, Microsoft Edge, Firefox, Brave, Opera, or any external browser's profile directory, cookies, `Cookies`, `Local State`, or `Login Data` files. Authentication happens only when you sign in directly on Twitch or Kick inside Dropilot's isolated WebView2 window.

## Deleting All Data

You can remove all Dropilot account sessions, WebView2 browsing data, and custom settings at any time via **Settings -> Accounts & Data -> Clear app data**, or by manually deleting `%APPDATA%\Dropilot`, `%LOCALAPPDATA%\Dropilot`, and the `Dropilot/*` entries in Windows Credential Manager.
