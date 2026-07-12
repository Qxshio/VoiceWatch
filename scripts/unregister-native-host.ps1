param(
    [ValidateSet("Chrome", "Edge", "Brave", "Vivaldi", "Opera", "Chromium", "Firefox", "All", "Both")]
    [string] $Browser = "All",

    [switch] $RemoveManifest
)

$ErrorActionPreference = "Stop"

$hostName = "com.voice_watch.native"
$manifestDir = Join-Path $env:LOCALAPPDATA "VoiceWatch\native-messaging"

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
        return @(
            $paths.Chrome,
            $paths.Edge,
            $paths.Brave,
            $paths.Vivaldi,
            $paths.Opera,
            $paths.Chromium,
            $paths.Firefox
        ) | Select-Object -Unique
    }

    return @($paths[$TargetBrowser]) | Select-Object -Unique
}

foreach ($registryPath in (Get-BrowserRegistryPaths $Browser)) {
    Remove-BrowserHost $registryPath
}

if ($RemoveManifest) {
    $manifestNames = if ($Browser -eq "Firefox") {
        @("$hostName-firefox.json")
    }
    elseif ($Browser -eq "All") {
        @("$hostName.json", "$hostName-firefox.json")
    }
    else {
        @("$hostName.json")
    }

    foreach ($manifestName in $manifestNames) {
        $manifestPath = Join-Path $manifestDir $manifestName
        if (Test-Path -LiteralPath $manifestPath) {
            Remove-Item -LiteralPath $manifestPath -Force
        }
    }
}

Write-Host "Unregistered $hostName"
