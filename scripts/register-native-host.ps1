param(
    [ValidateSet("Chrome", "Edge", "Both")]
    [string] $Browser = "Both",

    [Parameter(Mandatory = $true)]
    [ValidatePattern("^[a-p]{32}$")]
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

$manifestDir = Join-Path $env:LOCALAPPDATA "VoiceWatch\native-messaging"
$manifestPath = Join-Path $manifestDir "$hostName.json"
New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null

$manifest = [ordered]@{
    name = $hostName
    description = "Voice Watch native messaging host"
    path = $resolvedExePath
    type = "stdio"
    allowed_origins = @("chrome-extension://$ExtensionId/")
}

$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

function Register-BrowserHost {
    param([string] $RegistryPath)

    New-Item -Path $RegistryPath -Force | Out-Null
    Set-Item -Path $RegistryPath -Value $manifestPath
}

if ($Browser -in @("Chrome", "Both")) {
    Register-BrowserHost "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
}

if ($Browser -in @("Edge", "Both")) {
    Register-BrowserHost "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
}

Write-Host "Registered $hostName"
Write-Host "Manifest: $manifestPath"
Write-Host "Executable: $resolvedExePath"
