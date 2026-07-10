param(
    [string] $LogoPath,
    [string] $OutputDir
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$extensionDir = Join-Path $repoRoot "extension"
$cargoToml = Join-Path $repoRoot "Cargo.toml"
$cargoText = Get-Content -Raw -LiteralPath $cargoToml

if ([string]::IsNullOrWhiteSpace($LogoPath)) {
    $LogoPath = Join-Path $repoRoot "assets\logo.svg"
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $repoRoot "dist"
}

if ($cargoText -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw "Could not determine package version from Cargo.toml"
}

$version = $Matches[1]
$iconsDir = Join-Path $extensionDir "icons"
$stageRoot = Join-Path $repoRoot "target\extension-packages"
$chromiumStage = Join-Path $stageRoot "chromium"
$firefoxStage = Join-Path $stageRoot "firefox"
$firefoxSourceStage = Join-Path $stageRoot "firefox-source"

function New-CleanDirectory {
    param([string] $Path)
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

function Initialize-PackageOutput {
    param([string] $Path)
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
    Get-ChildItem -LiteralPath $Path -File -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Name -like "voice-watch-connector-*.zip" -or
            $_.Name -eq "STORE_EXTENSION_SHA256SUMS.txt"
        } |
        Remove-Item -Force
}

function Export-ExtensionIcons {
    param(
        [string] $SourceLogo,
        [string] $Destination
    )

    if (-not (Test-Path -LiteralPath $SourceLogo)) {
        throw "Logo was not found: $SourceLogo"
    }

    $svg = Get-Content -Raw -LiteralPath $SourceLogo
    $match = [regex]::Match($svg, 'href="data:image/png;base64,([^"]+)"')
    if (-not $match.Success) {
        throw "Logo SVG must contain an embedded PNG data image."
    }

    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    Copy-Item -LiteralPath $SourceLogo -Destination (Join-Path $Destination "logo.svg") -Force

    Add-Type -AssemblyName System.Drawing
    $bytes = [Convert]::FromBase64String($match.Groups[1].Value)
    $stream = [System.IO.MemoryStream]::new($bytes)
    $source = [System.Drawing.Image]::FromStream($stream)
    $canvas = [System.Drawing.Bitmap]::new(1024, 1024, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($canvas)
    $graphics.Clear([System.Drawing.Color]::Transparent)
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $graphics.DrawImage($source, 189, 1, 647, 1024)
    $graphics.Dispose()
    $source.Dispose()
    $stream.Dispose()

    foreach ($size in @(16, 32, 48, 96, 128, 256)) {
        $icon = [System.Drawing.Bitmap]::new($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $iconGraphics = [System.Drawing.Graphics]::FromImage($icon)
        $iconGraphics.Clear([System.Drawing.Color]::Transparent)
        $iconGraphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $iconGraphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $iconGraphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
        $iconGraphics.DrawImage($canvas, 0, 0, $size, $size)
        $iconGraphics.Dispose()
        $icon.Save((Join-Path $Destination "icon-$size.png"), [System.Drawing.Imaging.ImageFormat]::Png)
        $icon.Dispose()
    }

    $canvas.Dispose()
}

function Copy-ExtensionSource {
    param([string] $Destination)
    New-CleanDirectory $Destination
    Get-ChildItem -LiteralPath $extensionDir -Force |
        Where-Object { $_.Name -ne ".DS_Store" } |
        ForEach-Object {
            Copy-Item -LiteralPath $_.FullName -Destination $Destination -Recurse -Force
        }
}

function Copy-SourcePackagePath {
    param(
        [string] $RelativePath,
        [string] $DestinationRoot
    )

    $source = Join-Path $repoRoot $RelativePath
    if (-not (Test-Path -LiteralPath $source)) {
        throw "Source package input was not found: $RelativePath"
    }

    $destination = Join-Path $DestinationRoot $RelativePath
    $item = Get-Item -LiteralPath $source
    if ($item.PSIsContainer) {
        $parent = Split-Path -Parent $destination
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
        Copy-Item -LiteralPath $source -Destination $parent -Recurse -Force
        return
    }

    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath $source -Destination $destination -Force
}

function Copy-FirefoxSourcePackage {
    param([string] $Destination)

    New-CleanDirectory $Destination
    $sourceReadme = Join-Path $repoRoot "AMO_SOURCE_README.md"
    if (-not (Test-Path -LiteralPath $sourceReadme)) {
        $sourceReadme = Join-Path $repoRoot "README.md"
    }

    Copy-Item `
        -LiteralPath $sourceReadme `
        -Destination (Join-Path $Destination "README.md") `
        -Force

    $sourcePackagePaths = @(
        "Cargo.toml",
        "LICENSE",
        "assets",
        "extension",
        "scripts\package-extensions.ps1"
    )
    if (Test-Path -LiteralPath (Join-Path $repoRoot "AMO_SOURCE_README.md")) {
        $sourcePackagePaths = @("AMO_SOURCE_README.md") + $sourcePackagePaths
    }

    foreach ($path in $sourcePackagePaths) {
        Copy-SourcePackagePath -RelativePath $path -DestinationRoot $Destination
    }
}

function Write-FirefoxManifest {
    param([string] $Destination)

    $manifestPath = Join-Path $Destination "manifest.json"
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    $manifest.background = [ordered]@{
        scripts = @("service_worker.js")
    }
    $manifest | Add-Member -NotePropertyName browser_specific_settings -NotePropertyValue ([ordered]@{
        gecko = [ordered]@{
            id = "voice-watch-connector@qxshio.github.io"
            strict_min_version = "142.0"
            data_collection_permissions = [ordered]@{
                required = @("none")
            }
        }
    }) -Force
    $manifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
}

function New-ExtensionZip {
    param(
        [string] $SourceDirectory,
        [string] $DestinationZip
    )

    if (Test-Path -LiteralPath $DestinationZip) {
        Remove-Item -LiteralPath $DestinationZip -Force
    }

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $sourceRoot = (Resolve-Path -LiteralPath $SourceDirectory).Path.TrimEnd('\', '/')
    $zip = [System.IO.Compression.ZipFile]::Open(
        $DestinationZip,
        [System.IO.Compression.ZipArchiveMode]::Create
    )
    try {
        Get-ChildItem -LiteralPath $sourceRoot -File -Recurse |
            Sort-Object FullName |
            ForEach-Object {
                $relative = $_.FullName.Substring($sourceRoot.Length + 1).Replace('\', '/')
                [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                    $zip,
                    $_.FullName,
                    $relative,
                    [System.IO.Compression.CompressionLevel]::Optimal
                ) | Out-Null
            }
    }
    finally {
        $zip.Dispose()
    }
}

Export-ExtensionIcons -SourceLogo $LogoPath -Destination $iconsDir
Initialize-PackageOutput $OutputDir
New-CleanDirectory $stageRoot

Copy-ExtensionSource $chromiumStage
Copy-ExtensionSource $firefoxStage
Write-FirefoxManifest $firefoxStage

$chromeZip = Join-Path $OutputDir "voice-watch-connector-chrome-$version.zip"
$edgeZip = Join-Path $OutputDir "voice-watch-connector-edge-$version.zip"
$firefoxZip = Join-Path $OutputDir "voice-watch-connector-firefox-$version.zip"
$firefoxSourceZip = Join-Path $OutputDir "voice-watch-connector-firefox-source-$version.zip"

New-ExtensionZip -SourceDirectory $chromiumStage -DestinationZip $chromeZip
Copy-Item -LiteralPath $chromeZip -Destination $edgeZip -Force
New-ExtensionZip -SourceDirectory $firefoxStage -DestinationZip $firefoxZip
Copy-FirefoxSourcePackage $firefoxSourceStage
New-ExtensionZip -SourceDirectory $firefoxSourceStage -DestinationZip $firefoxSourceZip

Get-ChildItem -LiteralPath $OutputDir -File |
    Where-Object { $_.Name -like "voice-watch-connector-*.zip" } |
    ForEach-Object {
        $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName
        "$($hash.Hash.ToLowerInvariant())  $($_.Name)"
    } | Set-Content -LiteralPath (Join-Path $OutputDir "STORE_EXTENSION_SHA256SUMS.txt") -Encoding ASCII

Write-Host "Built extension upload packages in $OutputDir"
