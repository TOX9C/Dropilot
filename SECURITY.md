# Security Policy & Safety Commitments

## Reporting a Vulnerability

If you discover a security or privacy issue in Dropilot (such as an allowlist bypass, secret leakage in logs, or unsafe IPC exposure), please open a private security advisory on the repository or contact the maintainers directly rather than posting exploit details publicly.

Please include:
- Dropilot version and Windows version
- Clear reproduction steps or affected file/function
- Impact assessment

## Non-Negotiable Safety Guarantees

Dropilot is engineered to operate strictly within standard Windows user boundaries so neither users nor endpoint security tools have reason to distrust it:

1. **No Process Manipulation:** Zero code injection, DLL injection, memory patching, or anti-debug techniques.
2. **No Obfuscation or Packing:** All source code is plain and readable. Release binaries are standard MSVC builds without UPX or self-extracting packers.
3. **No External Browser Profile Access:** Dropilot never reads Chrome, Edge, Firefox, Brave, or Opera profiles, cookie stores, or saved passwords from disk. Session cookies are read only from Dropilot's own isolated WebView2 profile (`%LOCALAPPDATA%\Dropilot\WebView2\`) via Tauri's official `cookies_for_url` API right after user login.
4. **No Anti-Abuse Forgery:** Dropilot uses only official public web clients and never generates, modifies, or replays integrity tokens, Cloudflare clearances, or CAPTCHA solutions. When a platform requests verification or blocks a claim, Dropilot pauses and prompts the user to act inside the official window.
5. **Least Privilege:** Runs with `asInvoker` (no UAC elevation), installs per-user (`currentUser`), writes only inside `%APPDATA%\Dropilot` and `%LOCALAPPDATA%\Dropilot`, and uses HKCU Run only when the user explicitly enables **Start with Windows**.
6. **Allowlisted Network Traffic Only:** Every outbound HTTP and WebSocket request passes through `src-tauri/src/net/client.rs` and `src-tauri/src/net/allowlist.rs`. The frontend makes no network calls directly (enforced by CSP).
7. **Secret Hygiene:** Session tokens are stored only in Windows Credential Manager (`Dropilot/twitch`, `Dropilot/kick`), never persisted to settings or frontend state, and scrubbed from logs via `src-tauri/src/net/redact.rs`.
