# Privacy Policy & Local Storage Map

DropPilot collects **zero telemetry**, **zero analytics**, and **zero crash reports**. Nothing leaves your PC except direct requests to the official Twitch/Kick endpoints (and GitHub's release API if you manually enable or trigger an update check).

## Exact Storage Locations on Windows

DropPilot writes strictly inside per-user AppData folders and Windows Credential Manager:

| Data | Location | Notes |
| --- | --- | --- |
| **Session Tokens** | Windows Credential Manager (`DropPilot/twitch`, `DropPilot/kick`) | Protected by Windows DPAPI; never written to `settings.json`, logs, or frontend state |
| **App Settings** | `%APPDATA%\DropPilot\settings.json` | Written atomically (`.tmp` + sync + rename); contains theme, intervals, campaign toggles, priority list, and window bounds |
| **Rolling Logs** | `%LOCALAPPDATA%\DropPilot\logs\droppilot.log*` | Daily rolling, max 7 files, capped around 5 MB each; all tokens/cookies/auth headers scrubbed via `redact()` |
| **Isolated WebView2 Profile** | `%LOCALAPPDATA%\DropPilot\WebView2\` | Used strictly by DropPilot's own embedded login/inventory/UI windows |
| **Autostart (Optional, Off by Default)** | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Managed by `tauri-plugin-autostart`; never touches HKLM, Task Scheduler, or Windows Services |

## Browser Profile Isolation

DropPilot **never** accesses, reads, or modifies Chrome, Microsoft Edge, Firefox, Brave, Opera, or any external browser's profile directory, cookies, `Cookies`, `Local State`, or `Login Data` files. Authentication happens only when you sign in directly on Twitch or Kick inside DropPilot's isolated WebView2 window.

## Deleting All Data

You can remove all DropPilot account sessions, WebView2 browsing data, and custom settings at any time via **Settings -> Accounts & Data -> Clear app data**, or by manually deleting `%APPDATA%\DropPilot`, `%LOCALAPPDATA%\DropPilot`, and the `DropPilot/*` entries in Windows Credential Manager.
