param(
    [string] $IsccPath,
    [ValidatePattern("^[a-p]{32}$")]
    [string] $ExtensionId,
    [switch] $SkipRustBuild
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$cargoToml = Join-Path $repoRoot "Cargo.toml"
$cargoText = Get-Content -Raw -LiteralPath $cargoToml

if ($cargoText -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw "Could not determine package version from Cargo.toml"
}

$version = $Matches[1]
$distDir = Join-Path $repoRoot "dist"
$installerScript = Join-Path $repoRoot "installer\voice-watch.iss"
$releaseExe = Join-Path $repoRoot "target\release\voice-watch.exe"

if (-not $SkipRustBuild) {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if ($cargo) {
        $cargoPath = $cargo.Source
    }
    else {
        $cargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    }

    if (-not (Test-Path -LiteralPath $cargoPath)) {
        throw "Cargo was not found. Install Rust or add Cargo to PATH."
    }

    & $cargoPath build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed"
    }
}

if (-not (Test-Path -LiteralPath $releaseExe)) {
    throw "Release executable not found: $releaseExe"
}

New-Item -ItemType Directory -Force -Path $distDir | Out-Null
Copy-Item -LiteralPath $releaseExe -Destination (Join-Path $distDir "voice-watch-windows-x64.exe") -Force

if ([string]::IsNullOrWhiteSpace($IsccPath)) {
    $isccCommand = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($isccCommand) {
        $IsccPath = $isccCommand.Source
    }
}

if ([string]::IsNullOrWhiteSpace($IsccPath)) {
    $programFilesX86 = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFilesX86)
    $programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
    $candidates = @(
        (Join-Path $programFilesX86 "Inno Setup 6\ISCC.exe"),
        (Join-Path $programFiles "Inno Setup 6\ISCC.exe"),
        (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
        (Join-Path $env:ProgramData "chocolatey\bin\ISCC.exe")
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

    if (-not [string]::IsNullOrWhiteSpace($env:ChocolateyInstall)) {
        $candidates += (Join-Path $env:ChocolateyInstall "bin\ISCC.exe")
        $candidates += (Join-Path $env:ChocolateyInstall "lib\innosetup\tools\ISCC.exe")
    }

    $IsccPath = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}

if ([string]::IsNullOrWhiteSpace($IsccPath) -or -not (Test-Path -LiteralPath $IsccPath)) {
    throw "Inno Setup Compiler was not found. Install Inno Setup 6 or pass -IsccPath."
}

$isccArgs = @(
    "/DMyAppVersion=$version",
    $installerScript
)

if (-not [string]::IsNullOrWhiteSpace($ExtensionId)) {
    $isccArgs = @("/DExtensionId=$ExtensionId") + $isccArgs
}

& $IsccPath @isccArgs
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup build failed"
}

Write-Host "Built Voice Watch $version artifacts in $distDir"
