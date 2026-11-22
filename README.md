# Dropilot

Dropilot is a lightweight, modern Windows desktop application that automatically tracks and farms **Twitch Drops** and **Kick Drops** for your own logged-in accounts.

> **Disclaimer:** Using automation tools may violate Twitch or Kick Terms of Service. You are solely responsible for your own accounts. Dropilot is an independent utility and is not affiliated with, sponsored by, or endorsed by Twitch Interactive, Inc. or Kick Streaming Pty Ltd.

## Why this stack

Dropilot needs a modern UI and a real browser engine for the platforms' official login pages, while staying tiny and idle-cheap.

- Electron bundles a whole Chromium, so it ships at 100 MB+ and uses a lot of RAM.
- Python/PyInstaller builds are heavy and are well known for antivirus false positives.
- WPF/WinUI need either a .NET runtime or large self-contained bundles, and still need WebView2 for the login flow.
- Pure-native Rust GUIs (egui/iced) are light but would still need WebView2 for login, and take more work to make look modern.

Tauri 2 reuses the WebView2 runtime that is already on Windows, for both the UI and the official login window. The core is a small compiled Rust binary with a tiny idle footprint. The result is a few-MB per-user installer with a modern, fully custom UI.

## Key Features

- **Official WebView2 Login Only:** No username/password fields inside Dropilot. Login uses the official site inside Dropilot's isolated WebView2 window, and tokens are saved strictly in Windows Credential Manager.
- **Twitch & Kick Support:** Tracks active and upcoming drop campaigns, game-account link status, per-drop watch progress, and automatic claiming.
- **Priority Queue:** Drag-and-drop and keyboard-accessible ordering by game or campaign, plus per-channel pinning and hysteresis-guarded switching.
- **Tray-First Footprint:** Closing the main window to the system tray destroys the UI WebView2 window while keeping the lightweight Rust core active, and recreates the window on demand.
- **Mock Platform Mode:** Compile with `--features mock` to test and demonstrate the entire UI, state machine, selection, progress, and claims completely offline.
- **Defender-Friendly & Transparent:** Plain readable Rust/TypeScript, strict host allowlist, `asInvoker` manifest, per-user NSIS installer (`currentUser`), zero packers/UPX, zero telemetry, and zero browser-profile access.

## Build and Run

### Prerequisites (Windows 10 22H2+ / Windows 11)

1. **Rust stable toolchain** (`rustup` + `x86_64-pc-windows-msvc`)
2. **Microsoft C++ Build Tools** (Desktop development with C++)
3. **Node.js LTS** (v20+ / npm)
4. **Microsoft Edge WebView2 Runtime** (pre-installed on Windows 10 22H2+ and Windows 11)

### Development

```powershell
npm install

# Frontend typecheck & bundle check
npm run check
npm run build

# Run desktop app in offline Mock Mode (no accounts or network required)
npm run tauri:dev:mock

# Run desktop app with live platform clients
npm run tauri:dev
```

### Rust Verification

```powershell
cd src-tauri
cargo check
cargo clippy -- -D warnings
cargo test
```

### Production Build & Packaging

```powershell
npm run tauri:build
```

See [docs/PACKAGING.md](docs/PACKAGING.md) for portable ZIP creation, Authenticode signing, SHA-256 checksums, and Microsoft false-positive submission steps.

## Privacy & Security

- Tokens are stored only in Windows Credential Manager (`Dropilot/twitch`, `Dropilot/kick`).
- Settings live at `%APPDATA%\Dropilot\settings.json`; rolling logs live at `%LOCALAPPDATA%\Dropilot\logs\`.
- Dropilot's isolated WebView2 profile lives at `%LOCALAPPDATA%\Dropilot\WebView2\` and never touches Chrome, Edge, Firefox, Brave, or Opera profile folders.
- See [docs/PRIVACY.md](docs/PRIVACY.md), [docs/ENDPOINTS.md](docs/ENDPOINTS.md), and [SECURITY.md](SECURITY.md) for complete details.

## License

[MIT](LICENSE)
