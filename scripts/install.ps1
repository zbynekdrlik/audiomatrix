#Requires -Version 5.1
<#
.SYNOPSIS
    AudioMatrix Windows Installer
.DESCRIPTION
    Downloads and installs the latest AudioMatrix release from GitHub.
.EXAMPLE
    irm https://raw.githubusercontent.com/zbynekdrlik/audiomatrix/main/scripts/install.ps1 | iex
#>

$ErrorActionPreference = "Stop"

# Configuration
$repo = "zbynekdrlik/audiomatrix"
$installDir = "$env:LOCALAPPDATA\AudioMatrix"
$binName = "audiomatrix.exe"

function Write-Header {
    Write-Host ""
    Write-Host "=============================================" -ForegroundColor Cyan
    Write-Host "       AudioMatrix Installer" -ForegroundColor Cyan
    Write-Host "=============================================" -ForegroundColor Cyan
    Write-Host ""
}

function Get-LatestVersion {
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing
        return @{
            Version = $release.tag_name -replace '^v', ''
            Assets = $release.assets
        }
    }
    catch {
        throw "Failed to fetch latest release: $_"
    }
}

function Get-InstalledVersion {
    $exePath = Join-Path $installDir $binName
    if (Test-Path $exePath) {
        try {
            $output = & $exePath --version 2>&1
            if ($output -match '(\d+\.\d+\.\d+)') {
                return $Matches[1]
            }
        }
        catch {}
    }
    return $null
}

function Install-AudioMatrix {
    param(
        [string]$Version,
        [array]$Assets
    )

    # Find Windows x64 asset
    $asset = $Assets | Where-Object { $_.name -like "*windows-x64.zip" } | Select-Object -First 1
    if (-not $asset) {
        throw "No Windows x64 release found"
    }

    $zipUrl = $asset.browser_download_url
    $zipFile = Join-Path $env:TEMP "audiomatrix-$Version.zip"
    $extractDir = Join-Path $env:TEMP "audiomatrix-extract"

    Write-Host "Downloading AudioMatrix v$Version..." -ForegroundColor Yellow
    Invoke-WebRequest -Uri $zipUrl -OutFile $zipFile -UseBasicParsing

    # Download and verify checksum
    $sha256Asset = $Assets | Where-Object { $_.name -like "*windows-x64.zip.sha256" } | Select-Object -First 1
    if ($sha256Asset) {
        Write-Host "Verifying checksum..." -ForegroundColor Yellow
        $checksumContent = (Invoke-WebRequest -Uri $sha256Asset.browser_download_url -UseBasicParsing).Content
        $expectedHash = ($checksumContent -split '\s+')[0].Trim().ToLower()
        $actualHash = (Get-FileHash -Path $zipFile -Algorithm SHA256).Hash.ToLower()

        if ($expectedHash -ne $actualHash) {
            Remove-Item $zipFile -Force
            throw "Checksum verification failed! Expected: $expectedHash, Got: $actualHash"
        }
        Write-Host "Checksum verified!" -ForegroundColor Green
    }

    # Extract
    Write-Host "Extracting..." -ForegroundColor Yellow
    if (Test-Path $extractDir) {
        Remove-Item $extractDir -Recurse -Force
    }
    Expand-Archive -Path $zipFile -DestinationPath $extractDir -Force

    # Install
    Write-Host "Installing to $installDir..." -ForegroundColor Yellow
    if (-not (Test-Path $installDir)) {
        New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    }

    $sourceExe = Get-ChildItem -Path $extractDir -Filter "*.exe" -Recurse | Select-Object -First 1
    if (-not $sourceExe) {
        throw "No executable found in archive"
    }
    Copy-Item $sourceExe.FullName (Join-Path $installDir $binName) -Force

    # Cleanup
    Remove-Item $zipFile -Force -ErrorAction SilentlyContinue
    Remove-Item $extractDir -Recurse -Force -ErrorAction SilentlyContinue

    return Join-Path $installDir $binName
}

function Add-ToPath {
    param([string]$Dir)

    $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($userPath -notlike "*$Dir*") {
        Write-Host "Adding to PATH..." -ForegroundColor Yellow
        [Environment]::SetEnvironmentVariable("PATH", "$userPath;$Dir", "User")
        $env:PATH = "$env:PATH;$Dir"
        Write-Host "Added $Dir to PATH" -ForegroundColor Green
    }
}

function New-StartMenuShortcut {
    param([string]$ExePath)

    $startMenu = [Environment]::GetFolderPath("StartMenu")
    $shortcutPath = Join-Path $startMenu "Programs\AudioMatrix.lnk"

    try {
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $ExePath
        $shortcut.WorkingDirectory = $installDir
        $shortcut.Description = "AudioMatrix - Professional Audio Routing"
        $shortcut.Save()
        Write-Host "Created Start Menu shortcut" -ForegroundColor Green
    }
    catch {
        Write-Host "Warning: Could not create Start Menu shortcut: $_" -ForegroundColor Yellow
    }
}

# Main
try {
    Write-Header

    # Get latest version
    $release = Get-LatestVersion
    $latestVersion = $release.Version

    Write-Host "Latest version: v$latestVersion" -ForegroundColor Cyan

    # Check installed version
    $installedVersion = Get-InstalledVersion
    if ($installedVersion) {
        Write-Host "Installed version: v$installedVersion" -ForegroundColor Cyan

        if ($installedVersion -eq $latestVersion) {
            Write-Host ""
            Write-Host "AudioMatrix is already up to date!" -ForegroundColor Green
            Write-Host ""
            exit 0
        }

        Write-Host "Updating from v$installedVersion to v$latestVersion..." -ForegroundColor Yellow
    }
    else {
        Write-Host "No previous installation found" -ForegroundColor Gray
    }

    Write-Host ""

    # Install
    $exePath = Install-AudioMatrix -Version $latestVersion -Assets $release.Assets

    # Add to PATH
    Add-ToPath -Dir $installDir

    # Create shortcut
    New-StartMenuShortcut -ExePath $exePath

    Write-Host ""
    Write-Host "=============================================" -ForegroundColor Green
    Write-Host "  AudioMatrix v$latestVersion installed!" -ForegroundColor Green
    Write-Host "=============================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Run 'audiomatrix --help' to get started." -ForegroundColor Cyan
    Write-Host ""
}
catch {
    Write-Host ""
    Write-Host "Installation failed: $_" -ForegroundColor Red
    Write-Host ""
    exit 1
}
