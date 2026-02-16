#!/usr/bin/env pwsh
# ironclaw-installer.ps1
# Install IronClaw on Windows via PowerShell
#
# Usage:
#   irm https://github.com/danielsimonjr/ironclaw/releases/latest/download/ironclaw-installer.ps1 | iex
#
# Options (when running as a saved script):
#   .\ironclaw-installer.ps1                          # Install latest to default location
#   .\ironclaw-installer.ps1 -Version 0.1.3           # Install specific version
#   .\ironclaw-installer.ps1 -InstallDir "C:\tools"   # Custom install directory
#   .\ironclaw-installer.ps1 -NoPathUpdate             # Skip PATH modification
#   .\ironclaw-installer.ps1 -UseMsi                   # Use MSI installer instead

param(
    [string]$Version = "",
    [string]$InstallDir = "",
    [switch]$NoPathUpdate,
    [switch]$UseMsi
)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# ─── Configuration ───────────────────────────────────────────────────────────
$REPO_OWNER = "danielsimonjr"
$REPO_NAME = "ironclaw"
$BIN_NAME = "ironclaw"
$GITHUB_API = "https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases"
$GITHUB_DL = "https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download"

# ─── Helpers ─────────────────────────────────────────────────────────────────

function Write-Status($Message) {
    Write-Host "  > " -ForegroundColor Cyan -NoNewline
    Write-Host $Message
}

function Write-Success($Message) {
    Write-Host "  + " -ForegroundColor Green -NoNewline
    Write-Host $Message
}

function Write-Warn($Message) {
    Write-Host "  ! " -ForegroundColor Yellow -NoNewline
    Write-Host $Message
}

function Write-Fail($Message) {
    Write-Host "  x " -ForegroundColor Red -NoNewline
    Write-Host $Message
}

function Get-Architecture {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        "X64"   { return "x86_64" }
        "Arm64" { return "aarch64" }
        default {
            Write-Fail "Unsupported architecture: $arch"
            Write-Fail "IronClaw currently supports x86_64 (AMD64) Windows only."
            exit 1
        }
    }
}

function Get-LatestVersion {
    Write-Status "Fetching latest release version..."
    try {
        $release = Invoke-RestMethod -Uri "${GITHUB_API}/latest" -Headers @{
            "Accept" = "application/vnd.github.v3+json"
        }
        $tag = $release.tag_name
        # Strip leading 'v' if present
        if ($tag.StartsWith("v")) {
            $tag = $tag.Substring(1)
        }
        return $tag
    }
    catch {
        # Fallback: list releases and pick the first non-prerelease
        try {
            $releases = Invoke-RestMethod -Uri "${GITHUB_API}?per_page=10" -Headers @{
                "Accept" = "application/vnd.github.v3+json"
            }
            foreach ($rel in $releases) {
                if (-not $rel.prerelease -and -not $rel.draft) {
                    $tag = $rel.tag_name
                    if ($tag.StartsWith("v")) {
                        $tag = $tag.Substring(1)
                    }
                    return $tag
                }
            }
        }
        catch {
            Write-Fail "Failed to fetch release information from GitHub."
            Write-Fail "Error: $_"
            Write-Fail ""
            Write-Fail "Please check your internet connection and try again."
            Write-Fail "You can also download manually from:"
            Write-Fail "  https://github.com/${REPO_OWNER}/${REPO_NAME}/releases"
            exit 1
        }
        Write-Fail "No stable release found."
        exit 1
    }
}

function Get-DefaultInstallDir {
    # Prefer CARGO_HOME/bin if cargo is installed, otherwise use LocalAppData
    $cargoHome = $env:CARGO_HOME
    if (-not $cargoHome) {
        $cargoHome = Join-Path $env:USERPROFILE ".cargo"
    }
    $cargoBin = Join-Path $cargoHome "bin"

    if (Test-Path $cargoBin) {
        return $cargoBin
    }

    # Fallback to a dedicated install directory
    return Join-Path $env:LOCALAPPDATA "Programs\ironclaw\bin"
}

function Install-ViaMsi {
    param(
        [string]$TargetVersion,
        [string]$Arch
    )

    $target = "${Arch}-pc-windows-msvc"
    $msiName = "${BIN_NAME}-${TargetVersion}-${target}.msi"
    $msiUrl = "${GITHUB_DL}/v${TargetVersion}/${msiName}"
    $tempMsi = Join-Path $env:TEMP $msiName

    Write-Status "Downloading MSI installer..."
    Write-Status "  URL: $msiUrl"

    try {
        Invoke-WebRequest -Uri $msiUrl -OutFile $tempMsi -UseBasicParsing
    }
    catch {
        Write-Fail "Failed to download MSI installer."
        Write-Fail "Error: $_"
        Write-Fail ""
        Write-Fail "The MSI may not be available for this release."
        Write-Fail "Try installing without -UseMsi flag."
        exit 1
    }

    Write-Status "Running MSI installer..."
    Write-Status "  You may see a UAC prompt if installing system-wide."

    $msiArgs = "/i `"$tempMsi`" /passive /norestart"
    $process = Start-Process -FilePath "msiexec.exe" -ArgumentList $msiArgs -Wait -PassThru

    if ($process.ExitCode -ne 0) {
        Write-Fail "MSI installation failed with exit code: $($process.ExitCode)"
        exit 1
    }

    # Clean up
    Remove-Item -Path $tempMsi -Force -ErrorAction SilentlyContinue

    return $true
}

function Install-ViaArchive {
    param(
        [string]$TargetVersion,
        [string]$Arch,
        [string]$Destination
    )

    $target = "${Arch}-pc-windows-msvc"
    $archiveName = "${BIN_NAME}-${target}.tar.gz"
    $archiveUrl = "${GITHUB_DL}/v${TargetVersion}/${archiveName}"
    $tempArchive = Join-Path $env:TEMP $archiveName
    $tempExtract = Join-Path $env:TEMP "${BIN_NAME}-extract-$(Get-Random)"

    Write-Status "Downloading archive..."
    Write-Status "  URL: $archiveUrl"

    try {
        Invoke-WebRequest -Uri $archiveUrl -OutFile $tempArchive -UseBasicParsing
    }
    catch {
        Write-Fail "Failed to download archive."
        Write-Fail "Error: $_"
        Write-Fail ""
        Write-Fail "Please verify that release v${TargetVersion} exists at:"
        Write-Fail "  https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/tag/v${TargetVersion}"
        exit 1
    }

    Write-Status "Extracting..."

    # Create temp extraction directory
    New-Item -ItemType Directory -Force -Path $tempExtract | Out-Null

    # Extract tar.gz using built-in tar (available on Windows 10 1803+)
    try {
        tar -xzf $tempArchive -C $tempExtract 2>$null
    }
    catch {
        # Fallback: try using .NET for older Windows versions
        Write-Status "Falling back to .NET extraction..."
        try {
            Add-Type -AssemblyName System.IO.Compression.FileSystem

            # First decompress gzip
            $gzStream = [System.IO.File]::OpenRead($tempArchive)
            $decompressed = Join-Path $env:TEMP "${BIN_NAME}-temp.tar"
            $outStream = [System.IO.File]::Create($decompressed)
            $gzip = New-Object System.IO.Compression.GZipStream($gzStream, [System.IO.Compression.CompressionMode]::Decompress)
            $gzip.CopyTo($outStream)
            $gzip.Close()
            $outStream.Close()
            $gzStream.Close()

            # Then extract tar (basic implementation for single-binary archives)
            # Read tar and find the executable
            $tarBytes = [System.IO.File]::ReadAllBytes($decompressed)
            $offset = 0
            while ($offset -lt $tarBytes.Length) {
                # Read file name from tar header (first 100 bytes)
                $nameBytes = $tarBytes[$offset..($offset + 99)]
                $name = [System.Text.Encoding]::ASCII.GetString($nameBytes).Trim([char]0)
                if ([string]::IsNullOrEmpty($name)) { break }

                # Read file size from header (offset 124, 12 bytes, octal)
                $sizeStr = [System.Text.Encoding]::ASCII.GetString($tarBytes[($offset + 124)..($offset + 135)]).Trim([char]0).Trim()
                $size = [Convert]::ToInt64($sizeStr, 8)

                # Data starts after 512-byte header
                $dataOffset = $offset + 512

                if ($name -like "*${BIN_NAME}.exe") {
                    $exeBytes = $tarBytes[$dataOffset..($dataOffset + $size - 1)]
                    $exePath = Join-Path $tempExtract "${BIN_NAME}.exe"
                    [System.IO.File]::WriteAllBytes($exePath, $exeBytes)
                }

                # Next entry: header (512) + data rounded up to 512-byte boundary
                $offset = $dataOffset + [Math]::Ceiling($size / 512) * 512
            }

            Remove-Item -Path $decompressed -Force -ErrorAction SilentlyContinue
        }
        catch {
            Write-Fail "Failed to extract archive."
            Write-Fail "Error: $_"
            exit 1
        }
    }

    # Find the binary in extracted files
    $exePath = Get-ChildItem -Path $tempExtract -Recurse -Filter "${BIN_NAME}.exe" | Select-Object -First 1

    if (-not $exePath) {
        Write-Fail "Could not find ${BIN_NAME}.exe in the downloaded archive."
        exit 1
    }

    # Ensure destination directory exists
    if (-not (Test-Path $Destination)) {
        New-Item -ItemType Directory -Force -Path $Destination | Out-Null
        Write-Status "Created directory: $Destination"
    }

    # Copy binary to install directory
    $destBin = Join-Path $Destination "${BIN_NAME}.exe"

    # If existing binary is running, try to rename it first
    if (Test-Path $destBin) {
        $oldBin = Join-Path $Destination "${BIN_NAME}.old.exe"
        try {
            if (Test-Path $oldBin) {
                Remove-Item -Path $oldBin -Force
            }
            Rename-Item -Path $destBin -NewName "${BIN_NAME}.old.exe" -Force
        }
        catch {
            Write-Warn "Could not rename existing binary. It may be in use."
            Write-Warn "Please close any running IronClaw processes and try again."
            exit 1
        }
    }

    Copy-Item -Path $exePath.FullName -Destination $destBin -Force

    # Also copy updater if present
    $updaterPath = Get-ChildItem -Path $tempExtract -Recurse -Filter "${BIN_NAME}-update.exe" | Select-Object -First 1
    if ($updaterPath) {
        Copy-Item -Path $updaterPath.FullName -Destination (Join-Path $Destination "${BIN_NAME}-update.exe") -Force
    }

    # Clean up
    Remove-Item -Path $tempArchive -Force -ErrorAction SilentlyContinue
    Remove-Item -Path $tempExtract -Recurse -Force -ErrorAction SilentlyContinue

    return $destBin
}

function Add-ToUserPath {
    param(
        [string]$Directory
    )

    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($currentPath -split ";" | Where-Object { $_ -eq $Directory }) {
        Write-Status "Directory already in PATH."
        return
    }

    $newPath = "${currentPath};${Directory}"
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    $env:Path = "${env:Path};${Directory}"
    Write-Success "Added $Directory to user PATH."
    Write-Warn "Restart your terminal for PATH changes to take effect."
}

function Test-ExistingInstallation {
    $existing = Get-Command $BIN_NAME -ErrorAction SilentlyContinue
    if ($existing) {
        return $existing.Source
    }
    return $null
}

# ─── Main ────────────────────────────────────────────────────────────────────

function Install-IronClaw {
    Write-Host ""
    Write-Host "  IronClaw Installer for Windows" -ForegroundColor White
    Write-Host "  ===============================" -ForegroundColor DarkGray
    Write-Host ""

    # Check Windows version
    $osVersion = [Environment]::OSVersion.Version
    if ($osVersion.Major -lt 10) {
        Write-Fail "IronClaw requires Windows 10 or later."
        exit 1
    }

    # Detect architecture
    $arch = Get-Architecture
    Write-Status "Architecture: $arch"

    # Warn about ARM64 - only x86_64 is currently supported
    if ($arch -eq "aarch64") {
        Write-Warn "IronClaw does not currently have native ARM64 Windows builds."
        Write-Warn "The x86_64 build will be installed and run via Windows emulation."
        $arch = "x86_64"
    }

    # Determine version
    if ($Version) {
        $targetVersion = $Version
        if ($targetVersion.StartsWith("v")) {
            $targetVersion = $targetVersion.Substring(1)
        }
        Write-Status "Requested version: $targetVersion"
    }
    else {
        $targetVersion = Get-LatestVersion
    }
    Write-Success "Version: $targetVersion"

    # Check for existing installation
    $existing = Test-ExistingInstallation
    if ($existing) {
        Write-Warn "Existing installation found at: $existing"
        Write-Status "Upgrading..."
    }

    # MSI install path
    if ($UseMsi) {
        Write-Status "Installing via MSI..."
        Install-ViaMsi -TargetVersion $targetVersion -Arch $arch
        Write-Host ""
        Write-Success "IronClaw v${targetVersion} installed via MSI."
        Write-Host ""
        Write-Host "  Run 'ironclaw --help' to get started." -ForegroundColor DarkGray
        Write-Host ""
        return
    }

    # Determine install directory
    if ($InstallDir) {
        $installPath = $InstallDir
    }
    else {
        $installPath = Get-DefaultInstallDir
    }
    Write-Status "Install directory: $installPath"

    # Download and install
    $binPath = Install-ViaArchive -TargetVersion $targetVersion -Arch $arch -Destination $installPath

    # Update PATH
    if (-not $NoPathUpdate) {
        Add-ToUserPath -Directory $installPath
    }

    # Verify installation
    Write-Host ""
    if (Test-Path $binPath) {
        Write-Success "IronClaw v${targetVersion} installed successfully!"
        Write-Host ""
        Write-Host "  Binary: $binPath" -ForegroundColor DarkGray
        Write-Host ""
        Write-Host "  Run 'ironclaw --help' to get started." -ForegroundColor DarkGray
        Write-Host "  Run 'ironclaw onboard' for first-time setup." -ForegroundColor DarkGray
        Write-Host ""
    }
    else {
        Write-Fail "Installation may have failed. Binary not found at: $binPath"
        exit 1
    }
}

# Execute
Install-IronClaw
