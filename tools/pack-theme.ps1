param(
    [string]$Source = (Join-Path $PSScriptRoot '../examples/widget'),
    [string]$Output = (Join-Path $PSScriptRoot '../artifacts/example-widget.aidetheme')
)
$ErrorActionPreference = 'Stop'
$themeRoot = (Resolve-Path -LiteralPath $Source).Path
if (!(Test-Path -LiteralPath (Join-Path $themeRoot 'manifest.json')) -or !(Test-Path -LiteralPath (Join-Path $themeRoot 'index.html'))) {
    throw 'Theme root must contain manifest.json and index.html.'
}
$outputPath = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $outputPath) { throw "Output already exists: $outputPath" }
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($outputPath)) | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($themeRoot, $outputPath)
Write-Output $outputPath
