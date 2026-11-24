param(
    [string]$AppDir,
    [string]$MsixPath,
    [string]$ManifestPath,
    [switch]$Unregister
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

if ($Unregister) {
    $existing = Get-AppxPackage -Name 'io.dropilot.app' -ErrorAction SilentlyContinue
    if ($existing) {
        $existing | Remove-AppxPackage
        Write-Host "Unregistered Windows Package Identity: $($existing.PackageFullName)"
    } else {
        Write-Host "Windows Package Identity 'io.dropilot.app' is not currently registered."
    }
    return
}

if ([string]::IsNullOrWhiteSpace($AppDir)) {
    $installedDir = Join-Path $env:LOCALAPPDATA 'Dropilot'
    $releaseDir = Join-Path $repoRoot 'src-tauri\target\release'
    if (Test-Path (Join-Path $installedDir 'dropilot.exe')) {
        $AppDir = $installedDir
    } elseif (Test-Path (Join-Path $releaseDir 'dropilot.exe')) {
        $AppDir = $releaseDir
    } else {
        $AppDir = $installedDir
    }
}

$AppDir = [System.IO.Path]::GetFullPath($AppDir)
if (-not (Test-Path $AppDir)) {
    throw "AppDir does not exist: $AppDir"
}

if (-not (Test-Path (Join-Path $AppDir 'dropilot.exe'))) {
    Write-Warning "dropilot.exe was not found in '$AppDir'. Package Identity will bind to this external location anyway."
}

if ([string]::IsNullOrWhiteSpace($MsixPath)) {
    $installedMsix = Join-Path $AppDir 'packaging\DropilotIdentity.msix'
    $repoMsix = Join-Path $repoRoot 'src-tauri\packaging\DropilotIdentity.msix'
    if (Test-Path $installedMsix) {
        $MsixPath = $installedMsix
    } elseif (Test-Path $repoMsix) {
        $MsixPath = $repoMsix
    }
}

if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
    $installedManifest = Join-Path $AppDir 'packaging\msix\AppxManifest.xml'
    $repoManifest = Join-Path $repoRoot 'src-tauri\packaging\msix\AppxManifest.xml'
    if (Test-Path $installedManifest) {
        $ManifestPath = $installedManifest
    } else {
        $ManifestPath = $repoManifest
    }
}

$devModeEnabled = $false
try {
    $unlock = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue
    $devModeEnabled = ($unlock.AllowDevelopmentWithoutDevLicense -eq 1)
} catch {
    $devModeEnabled = $false
}

try {
    if (-not [string]::IsNullOrWhiteSpace($MsixPath) -and (Test-Path $MsixPath)) {
        $sig = Get-AuthenticodeSignature -FilePath $MsixPath -ErrorAction SilentlyContinue
        if ($sig -and $sig.Status -eq 'Valid') {
            Write-Host "Registering signed sparse MSIX '$MsixPath' -> ExternalLocation '$AppDir'"
            Add-AppxPackage -Path $MsixPath -ExternalLocation $AppDir -ForceUpdateFromAnyVersion -ErrorAction Stop
        } elseif ($devModeEnabled -and (Test-Path $ManifestPath)) {
            Write-Host "Developer Mode enabled; registering manifest '$ManifestPath' -> ExternalLocation '$AppDir'"
            Add-AppxPackage -Register $ManifestPath -ExternalLocation $AppDir -ForceUpdateFromAnyVersion -ErrorAction Stop
        } else {
            Write-Host "Attempting sparse MSIX registration '$MsixPath' -> ExternalLocation '$AppDir'"
            Add-AppxPackage -Path $MsixPath -ExternalLocation $AppDir -ForceUpdateFromAnyVersion -ErrorAction Stop
        }
    } elseif ($devModeEnabled -and (Test-Path $ManifestPath)) {
        Write-Host "Developer Mode enabled; registering manifest '$ManifestPath' -> ExternalLocation '$AppDir'"
        Add-AppxPackage -Register $ManifestPath -ExternalLocation $AppDir -ForceUpdateFromAnyVersion -ErrorAction Stop
    } else {
        throw "Neither a valid signed DropilotIdentity.msix nor Windows Developer Mode (for AppxManifest.xml registration) is available."
    }

    $pkg = Get-AppxPackage -Name 'io.dropilot.app' -ErrorAction SilentlyContinue
    if ($pkg) {
        Write-Host "Registered successfully: $($pkg.PackageFamilyName) (Version $($pkg.Version))"
    }
} catch {
    Write-Warning "Failed registering Dropilot Windows Package Identity: $($_.Exception.Message)"
    Write-Host ""
    Write-Host "Diagnostic guidance:"
    Write-Host "  1. Developer Mode is currently $(if ($devModeEnabled) { 'ENABLED' } else { 'DISABLED' })."
    Write-Host "     To test unsigned/dev builds locally, enable Settings -> System -> For developers -> Developer Mode."
    Write-Host "  2. If using a self-signed dev MSIX (-DevSelfSign), import the exported .cer into Cert:\LocalMachine\TrustedPeople (Admin PowerShell)."
    Write-Host "  3. For end-user machines without Developer Mode, sign DropilotIdentity.msix with a publicly trusted Authenticode / Azure Trusted Signing cert matching Publisher."
    exit 1
}
