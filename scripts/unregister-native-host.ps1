param(
    [ValidateSet("Chrome", "Edge", "Both")]
    [string] $Browser = "Both",

    [switch] $RemoveManifest
)

$ErrorActionPreference = "Stop"

$hostName = "com.voice_watch.native"
$manifestPath = Join-Path $env:LOCALAPPDATA "VoiceWatch\native-messaging\$hostName.json"

function Remove-BrowserHost {
    param([string] $RegistryPath)

    if (Test-Path -LiteralPath $RegistryPath) {
        Remove-Item -LiteralPath $RegistryPath -Recurse -Force
    }
}

if ($Browser -in @("Chrome", "Both")) {
    Remove-BrowserHost "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
}

if ($Browser -in @("Edge", "Both")) {
    Remove-BrowserHost "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
}

if ($RemoveManifest -and (Test-Path -LiteralPath $manifestPath)) {
    Remove-Item -LiteralPath $manifestPath -Force
}

Write-Host "Unregistered $hostName"
