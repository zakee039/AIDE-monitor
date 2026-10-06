param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    $version = (Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version
    $tauriVersion = (Get-Content -LiteralPath 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json).version
    if ($version -ne $tauriVersion) { throw 'Package and Tauri versions differ.' }
    if (-not $SkipBuild) {
        & npm.cmd run tauri build -- --bundles nsis
        if ($LASTEXITCODE -ne 0) { throw 'Application build failed.' }
    }
    $output = Join-Path $workspace "artifacts/release-v$version"
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $name = "AIDE-monitor-$version-windows-x64"
    Copy-Item -LiteralPath 'src-tauri/target/release/aide-monitor.exe' -Destination (Join-Path $output "$name.exe") -Force
    $installers = @(Get-ChildItem -LiteralPath 'src-tauri/target/release/bundle/nsis' -Filter "*_${version}_x64-setup.exe")
    if ($installers.Count -ne 1) { throw 'Expected exactly one installer for this version.' }
    Copy-Item -LiteralPath $installers[0].FullName -Destination (Join-Path $output "$name-setup.exe") -Force
    Compress-Archive -LiteralPath (Join-Path $output "$name.exe") -DestinationPath (Join-Path $output "$name.zip") -Force
    $files = @("$name.exe", "$name-setup.exe", "$name.zip")
    $checksums = foreach ($file in $files) {
        $hash = (Get-FileHash -LiteralPath (Join-Path $output $file) -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $file"
    }
    [IO.File]::WriteAllLines((Join-Path $output 'SHA256SUMS.txt'), $checksums, [Text.UTF8Encoding]::new($false))
    Get-ChildItem -LiteralPath $output | Select-Object Name, Length
} finally { Pop-Location }
