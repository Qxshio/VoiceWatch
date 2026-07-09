param(
    [ValidateSet("Chrome", "Edge", "Brave", "Vivaldi", "Opera", "Chromium", "All", "Both")]
    [string] $Browser = "All",

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

function Get-BrowserRegistryPaths {
    param([string] $TargetBrowser)

    $paths = [ordered]@{
        Chrome = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
        Edge = "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
        Brave = "HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\$hostName"
        Vivaldi = "HKCU:\Software\Vivaldi\NativeMessagingHosts\$hostName"
        Opera = @(
            "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName",
            "HKCU:\Software\Opera Software\Opera Stable\NativeMessagingHosts\$hostName"
        )
        Chromium = "HKCU:\Software\Chromium\NativeMessagingHosts\$hostName"
    }

    if ($TargetBrowser -eq "Both") {
        return @($paths.Chrome, $paths.Edge)
    }

    if ($TargetBrowser -eq "All") {
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
    Remove-BrowserHost $registryPath
}

if ($RemoveManifest -and (Test-Path -LiteralPath $manifestPath)) {
    Remove-Item -LiteralPath $manifestPath -Force
}

Write-Host "Unregistered $hostName"
