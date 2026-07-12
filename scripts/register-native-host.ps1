param(
    [ValidateSet("Chrome", "Edge", "Brave", "Vivaldi", "Opera", "Chromium", "Firefox", "All", "Both")]
    [string] $Browser = "All",

    [Parameter(Mandatory = $true)]
    [ValidateScript({
        $_ -match '^[a-p]{32}$' -or $_ -eq 'voice-watch-connector@qxshio.github.io'
    })]
    [string] $ExtensionId,

    [string] $ExePath
)

$ErrorActionPreference = "Stop"

$hostName = "com.voice_watch.native"

if ([string]::IsNullOrWhiteSpace($ExePath)) {
    $installedExe = Join-Path $PSScriptRoot "..\voice-watch.exe"
    $devExe = Join-Path $PSScriptRoot "..\target\release\voice-watch.exe"
    if (Test-Path -LiteralPath $installedExe) {
        $ExePath = $installedExe
    }
    else {
        $ExePath = $devExe
    }
}

$resolvedExePath = [System.IO.Path]::GetFullPath($ExePath)

if (-not (Test-Path -LiteralPath $resolvedExePath)) {
    throw "Executable not found: $resolvedExePath. Build the app first or pass -ExePath."
}

$firefoxExtensionId = "voice-watch-connector@qxshio.github.io"
$isFirefox = $ExtensionId -eq $firefoxExtensionId
if ($isFirefox -and $Browser -notin @("All", "Firefox")) {
    throw "The Firefox add-on ID can only be registered for Firefox."
}
if (-not $isFirefox -and $Browser -eq "Firefox") {
    throw "A Chromium extension ID cannot be registered for Firefox."
}

$manifestDir = Join-Path $env:LOCALAPPDATA "VoiceWatch\native-messaging"
$manifestName = if ($isFirefox) { "$hostName-firefox.json" } else { "$hostName.json" }
$manifestPath = Join-Path $manifestDir $manifestName
New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null

$allowedIds = @()
if (Test-Path -LiteralPath $manifestPath) {
    try {
        $existingManifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
        $existingIds = if ($isFirefox) {
            $existingManifest.allowed_extensions
        }
        else {
            $existingManifest.allowed_origins
        }
        if ($existingIds) {
            $allowedIds = @($existingIds)
        }
    }
    catch {
        $allowedIds = @()
    }
}
$allowedId = if ($isFirefox) { $ExtensionId } else { "chrome-extension://$ExtensionId/" }
$allowedIds = @($allowedIds + $allowedId) | Sort-Object -Unique

$manifest = [ordered]@{
    name = $hostName
    description = "Voice Watch native messaging host"
    path = $resolvedExePath
    type = "stdio"
}
if ($isFirefox) {
    $manifest["allowed_extensions"] = $allowedIds
}
else {
    $manifest["allowed_origins"] = $allowedIds
}

$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

function Register-BrowserHost {
    param([string] $RegistryPath)

    New-Item -Path $RegistryPath -Force | Out-Null
    Set-Item -Path $RegistryPath -Value $manifestPath
}

function Get-BrowserRegistryPaths {
    param([string] $TargetBrowser)

    $paths = [ordered]@{
        Chrome = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
        Edge = "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
        Brave = "HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\$hostName"
        Vivaldi = "HKCU:\Software\Vivaldi\NativeMessagingHosts\$hostName"
        Opera = @(
            "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName",
            "HKCU:\Software\Opera Software\Opera Stable\NativeMessagingHosts\$hostName",
            "HKCU:\Software\Opera Software\Opera GX Stable\NativeMessagingHosts\$hostName"
        )
        Chromium = "HKCU:\Software\Chromium\NativeMessagingHosts\$hostName"
        Firefox = "HKCU:\Software\Mozilla\NativeMessagingHosts\$hostName"
    }

    if ($TargetBrowser -eq "Both") {
        return @($paths.Chrome, $paths.Edge)
    }

    if ($TargetBrowser -eq "All") {
        if ($isFirefox) {
            return @($paths.Firefox)
        }
        return @(
            $paths.Chrome,
            $paths.Edge,
            $paths.Brave,
            $paths.Vivaldi,
            $paths.Opera,
            $paths.Chromium
        ) | Select-Object -Unique
    }

    return @($paths[$TargetBrowser]) | Select-Object -Unique
}

foreach ($registryPath in (Get-BrowserRegistryPaths $Browser)) {
    Register-BrowserHost $registryPath
}

Write-Host "Registered $hostName"
Write-Host "Manifest: $manifestPath"
Write-Host "Executable: $resolvedExePath"
