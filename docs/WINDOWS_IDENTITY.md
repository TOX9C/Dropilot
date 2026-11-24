# Windows Sparse MSIX Package Identity

Dropilot uses an external-location Windows Sparse MSIX Package (`io.dropilot.app`) so that `dropilot.exe` and its child `msedgewebview2.exe` processes group cleanly under **Dropilot** in Windows 11 Task Manager while continuing to ship as a standard per-user NSIS installer (`currentUser`).

## How It Works

1. **Embedded Win32 Manifest (`src-tauri/build.rs`)**:
   - Embeds `<assemblyIdentity ... name="io.dropilot.app" />` and `<msix xmlns="urn:schemas-microsoft-com:msix.v1" publisher="..." packageName="io.dropilot.app" applicationId="Dropilot" />`.
   - The `publisher` defaults to `CN=Dropilot` and can be overridden at build time via `DROPILOT_MSIX_PUBLISHER`.
2. **Sparse Package Manifest (`src-tauri/packaging/msix/AppxManifest.xml`)**:
   - Declares `<uap10:AllowExternalContent>true</uap10:AllowExternalContent>` and `<uap:VisualElements ... AppListEntry="none" />` so Windows grants Package Identity without duplicating Start Menu shortcuts.
3. **NSIS Installer Hooks (`src-tauri/installer/nsis-hooks.nsh`)**:
   - `NSIS_HOOK_POSTINSTALL` registers `$INSTDIR\packaging\DropilotIdentity.msix` with `-ExternalLocation "$INSTDIR"` (non-fatally, falling back to `AppxManifest.xml` when Windows Developer Mode is enabled).
   - `NSIS_HOOK_POSTUNINSTALL` removes `io.dropilot.app` cleanly.
4. **Runtime Verification**:
   - At startup, Dropilot logs `Running with Windows Package Identity` with its `package_family_name` (or `Running unpackaged without Windows Package Identity`).

## Certificate Requirements for End-User Machines

Windows requires a **trusted Authenticode signature** on `DropilotIdentity.msix` (and a matching `Publisher` Subject DN) to register a sparse MSIX package automatically during NSIS install on machines where Developer Mode is **disabled**:

- **Recommended for production releases**:
  - **Azure Trusted Signing** (formerly Microsoft Trusted Signing) or a standard **OV/EV Windows Code Signing Certificate** issued by a publicly trusted CA (e.g., DigiCert, Sectigo, GlobalSign).
- **GitHub Actions Secrets** (`.github/workflows/release.yml`):
  - `WINDOWS_SIGNING_CERT_BASE64`: Base64-encoded `.pfx` code-signing certificate.
  - `WINDOWS_SIGNING_CERT_PASSWORD`: Password for the `.pfx` file.
  - `WINDOWS_SIGNING_PUBLISHER` *(optional)*: Exact Subject DN (for example, `CN=Your Name, O=Your Org, C=US`). If omitted, CI reads `$cert.Subject` directly from the `.pfx` and sets `DROPILOT_MSIX_PUBLISHER` automatically so the Win32 embedded `<msix>` manifest and `AppxManifest.xml` match byte-for-byte.

> **Important**: Tauri's `TAURI_SIGNING_PRIVATE_KEY` (minisign) only signs auto-updater payloads; it is separate from Windows Authenticode signing. Without `WINDOWS_SIGNING_CERT_BASE64`, CI packs an unsigned `DropilotIdentity.msix` so the NSIS installer still succeeds cleanly, and Package Identity activates only on machines with Developer Mode enabled.

## Testing Locally on Windows 11

### Option A: Developer Mode (No Certificate Required)

1. Enable **Settings → System → For developers → Developer Mode**.
2. Build Dropilot (`npm run tauri:build` or `cargo build --manifest-path src-tauri/Cargo.toml --release`).
3. Register the sparse manifest against the binary folder:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/register-windows-identity.ps1 -AppDir ".\src-tauri\target\release"
```

### Option B: Self-Signed Dev Certificate

1. Build and sign a local `DropilotIdentity.msix`:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-sparse-msix.ps1 -DevSelfSign
```

2. Trust the exported `.cer` printed by the script (in an elevated PowerShell):

```powershell
Import-Certificate -FilePath "$env:TEMP\dropilot-dev-cert\dropilot-dev.cer" -CertStoreLocation Cert:\LocalMachine\TrustedPeople
```

3. Register against your installed or built folder:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/register-windows-identity.ps1 -AppDir "$env:LOCALAPPDATA\Dropilot"
```

### Unregistering

```powershell
powershell -ExecutionPolicy Bypass -File scripts/register-windows-identity.ps1 -Unregister
```
