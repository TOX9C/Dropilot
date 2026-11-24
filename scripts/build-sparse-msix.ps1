param(
    [string]$Publisher,
    [string]$Version,
    [string]$OutPath,
    [string]$PfxPath,
    [string]$PfxPassword,
    [switch]$DevSelfSign,
    [string]$DevCertOutDir
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$msixSourceDir = Join-Path $repoRoot 'src-tauri\packaging\msix'
$packageJsonPath = Join-Path $repoRoot 'package.json'

if (-not (Test-Path $msixSourceDir)) {
    throw "MSIX source directory not found: $msixSourceDir"
}

if ([string]::IsNullOrWhiteSpace($Publisher)) {
    if (-not [string]::IsNullOrWhiteSpace($env:DROPILOT_MSIX_PUBLISHER)) {
        $Publisher = $env:DROPILOT_MSIX_PUBLISHER.Trim()
    } else {
        $Publisher = 'CN=Dropilot'
    }
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    $pkg = Get-Content -Raw -Path $packageJsonPath | ConvertFrom-Json
    $Version = "$($pkg.version).0"
}

if ([string]::IsNullOrWhiteSpace($OutPath)) {
    $OutPath = Join-Path $repoRoot 'src-tauri\packaging\DropilotIdentity.msix'
}

$outDir = Split-Path -Parent $OutPath
if (-not [string]::IsNullOrWhiteSpace($outDir)) {
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
}

function Find-WindowsSdkTool([string]$ToolName) {
    $kitRoot = 'C:\Program Files (x86)\Windows Kits\10\bin'
    if (Test-Path $kitRoot) {
        $candidates = Get-ChildItem -Path $kitRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            ForEach-Object { Join-Path $_.FullName "x64\$ToolName" } |
            Where-Object { Test-Path $_ }
        if ($candidates) {
            return $candidates | Select-Object -First 1
        }
    }
    $cmd = Get-Command $ToolName -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }
    throw "Required Windows SDK tool '$ToolName' was not found. Install Windows 10/11 SDK."
}

$makeappx = Find-WindowsSdkTool 'makeappx.exe'

$stageDir = Join-Path $env:TEMP ("dropilot-msix-stage-" + [guid]::NewGuid().ToString('N'))
try {
    New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
    Copy-Item -Path (Join-Path $msixSourceDir '*') -Destination $stageDir -Recurse -Force

    $manifestPath = Join-Path $stageDir 'AppxManifest.xml'
    [xml]$manifest = Get-Content -Raw -Path $manifestPath -Encoding UTF8
    $manifest.Package.Identity.Publisher = $Publisher
    $manifest.Package.Identity.Version = $Version

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    $sw = New-Object System.IO.StreamWriter($manifestPath, $false, $utf8NoBom)
    try {
        $manifest.Save($sw)
    } finally {
        $sw.Dispose()
    }

    Write-Host "Packing sparse MSIX ($Version, Publisher='$Publisher') -> $OutPath"
    & $makeappx pack /d $stageDir /nv /o /p $OutPath
    if ($LASTEXITCODE -ne 0) {
        throw "makeappx.exe failed with exit code $LASTEXITCODE"
    }

    if (-not [string]::IsNullOrWhiteSpace($PfxPath)) {
        if (-not (Test-Path $PfxPath)) {
            throw "PFX certificate not found: $PfxPath"
        }
        $signtool = Find-WindowsSdkTool 'signtool.exe'
        $signArgs = @('sign', '/fd', 'SHA256', '/tr', 'http://timestamp.digicert.com', '/td', 'SHA256', '/f', $PfxPath)
        if (-not [string]::IsNullOrWhiteSpace($PfxPassword)) {
            $signArgs += @('/p', $PfxPassword)
        }
        $signArgs += $OutPath

        Write-Host "Signing sparse MSIX with $PfxPath"
        & $signtool @signArgs
        if ($LASTEXITCODE -ne 0) {
            throw "signtool.exe failed with exit code $LASTEXITCODE"
        }
    } elseif ($DevSelfSign) {
        $signtool = Find-WindowsSdkTool 'signtool.exe'
        if ([string]::IsNullOrWhiteSpace($DevCertOutDir)) {
            $DevCertOutDir = Join-Path $env:TEMP 'dropilot-dev-cert'
        }
        New-Item -ItemType Directory -Force -Path $DevCertOutDir | Out-Null

        $devPfxPath = Join-Path $DevCertOutDir 'dropilot-dev.pfx'
        $devCerPath = Join-Path $DevCertOutDir 'dropilot-dev.cer'
        $devPassPlain = 'dropilot-dev-local'
        $devPass = ConvertTo-SecureString -String $devPassPlain -Force -AsPlainText

        $cert = New-SelfSignedCertificate `
            -Type Custom `
            -Subject $Publisher `
            -KeyUsage DigitalSignature `
            -FriendlyName 'Dropilot Dev Sparse Identity' `
            -CertStoreLocation 'Cert:\CurrentUser\My' `
            -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')

        Export-PfxCertificate -Cert $cert -FilePath $devPfxPath -Password $devPass | Out-Null
        Export-Certificate -Cert $cert -FilePath $devCerPath | Out-Null

        Write-Host "Signing sparse MSIX with self-signed dev cert -> $devCerPath"
        & $signtool sign /fd SHA256 /f $devPfxPath /p $devPassPlain $OutPath
        if ($LASTEXITCODE -ne 0) {
            throw "signtool.exe dev self-sign failed with exit code $LASTEXITCODE"
        }

        Write-Host "Local dev certificate exported to: $devCerPath"
        Write-Host "To trust on this machine (Admin PowerShell): Import-Certificate -FilePath `"$devCerPath`" -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
    } else {
        Write-Host "Built unsigned sparse MSIX at $OutPath (suitable for Developer Mode or subsequent signing)."
    }
} finally {
    if (Test-Path $stageDir) {
        Remove-Item -Recurse -Force -Path $stageDir -ErrorAction SilentlyContinue
    }
}
