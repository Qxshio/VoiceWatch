param(
    [string] $ExePath
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
if ([string]::IsNullOrWhiteSpace($ExePath)) {
    $ExePath = Join-Path $repoRoot "target\release\voice-watch.exe"
}

$ExePath = (Resolve-Path -LiteralPath $ExePath).Path
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$probeDir = [IO.Path]::GetFullPath(
    (Join-Path $tempRoot "VoiceWatch-handoff-smoke-$([Guid]::NewGuid().ToString('N'))")
)

if (-not $probeDir.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use a probe directory outside the Windows temp directory."
}

New-Item -ItemType Directory -Path $probeDir | Out-Null

try {
    foreach ($name in @("voice-watch-handoff.exe", "voice-watch-updater.exe")) {
        $probeExe = Join-Path $probeDir $name
        Copy-Item -LiteralPath $ExePath -Destination $probeExe

        $startInfo = New-Object System.Diagnostics.ProcessStartInfo
        $startInfo.FileName = $probeExe
        $startInfo.Arguments = "--help"
        $startInfo.UseShellExecute = $false

        $process = [Diagnostics.Process]::Start($startInfo)
        if (-not $process.WaitForExit(10000)) {
            $process.Kill()
            throw "$name did not exit within 10 seconds."
        }

        $exitCode = $process.ExitCode
        $process.Dispose()
        if ($exitCode -ne 0) {
            throw "$name exited with code $exitCode."
        }

        Write-Host "Update handoff launch passed: $name"
    }
}
finally {
    if (
        (Test-Path -LiteralPath $probeDir) -and
        $probeDir.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)
    ) {
        Remove-Item -LiteralPath $probeDir -Recurse -Force
    }
}
