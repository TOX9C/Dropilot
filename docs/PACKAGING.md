# Packaging, Signing & Microsoft Defender Submission

Dropilot is built and distributed without packers, UPX, self-extracting Python wrappers, or elevation requirements.

## 1. Release Build (Per-User NSIS Installer)

From the project root (`dropilot/`):

```powershell
npm install
npm run check
npm run tauri:build
```

Outputs land in:
- **Executable:** `src-tauri\target\release\dropilot.exe`
- **Per-user NSIS installer:** `src-tauri\target\release\bundle\nsis\Dropilot_0.1.0_x64-setup.exe`

Configuration highlights in `src-tauri/tauri.conf.json` & `src-tauri/build.rs`:
- `bundle.targets`: `["nsis"]`
- `bundle.windows.nsis.installMode`: `"currentUser"` (installs into `%LOCALAPPDATA%` with zero UAC prompt)
- `bundle.windows.webviewInstallMode.type`: `"skip"` (never silently downloads a bootstrapper; if WebView2 is absent, `main.rs` asks the user before opening Microsoft's official WebView2 download page)
- Manifest: `<requestedExecutionLevel level="asInvoker" uiAccess="false" />`
- Full Windows version metadata populated (`productName`, `version`, `publisher`, `copyright`, `shortDescription`)

## 2. Portable ZIP Build

Create a plain portable ZIP folder directly from the release binary (no self-extractor):

```powershell
$portableDir = ".\dist-portable\Dropilot-0.1.0-win64"
New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
Copy-Item ".\src-tauri\target\release\dropilot.exe" "$portableDir\Dropilot.exe"
Copy-Item ".\LICENSE" "$portableDir\LICENSE.txt"
Copy-Item ".\README.md" "$portableDir\README.md"
Compress-Archive -Path "$portableDir\*" -DestinationPath ".\dist-portable\Dropilot-0.1.0-win64-portable.zip" -Force
```

## 3. SHA-256 Checksums

Generate checksums next to every release artifact:

```powershell
Get-FileHash `
  ".\src-tauri\target\release\bundle\nsis\Dropilot_0.1.0_x64-setup.exe", `
  ".\dist-portable\Dropilot-0.1.0-win64-portable.zip" `
  -Algorithm SHA256 | Format-List
```

## 4. Authenticode Code Signing

When an Authenticode or Azure Trusted Signing certificate is available:

1. Set `bundle.windows.certificateThumbprint` in `src-tauri/tauri.conf.json` (or configure `TAURI_SIGNING_PRIVATE_KEY` / custom `signCommand`).
2. Or sign both `dropilot.exe` (before bundling) and the NSIS setup executable directly with `signtool.exe`:

```powershell
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /sha1 <CERT_THUMBPRINT> ".\src-tauri\target\release\dropilot.exe"
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /sha1 <CERT_THUMBPRINT> ".\src-tauri\target\release\bundle\nsis\Dropilot_0.1.0_x64-setup.exe"
```

## 5. Submitting False Positives to Microsoft Defender (WDSI)

Unsigned Windows binaries can occasionally receive heuristic warnings until reputation builds. If Defender flags a clean release build:

1. Visit **https://www.microsoft.com/wdsi/filesubmission**.
2. Sign in with a Microsoft account and select **Software developer**.
3. Upload the signed/clean NSIS installer (`Dropilot_<version>_x64-setup.exe`) and/or `Dropilot.exe`.
4. Select **Incorrectly detected as malware/malicious**.
5. Provide the product name (`Dropilot`), version (`0.1.0`), official repository URL, and a brief description noting it is an open-source Tauri 2 desktop application.
6. After Microsoft updates signatures, run `mpcmdrun -SignatureUpdate` locally to verify clearance.
