param(
    [string]$ApplicationPath = (Join-Path $env:LOCALAPPDATA 'AIDE monitor\aide-monitor.exe')
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $ApplicationPath -PathType Leaf)) {
    throw "Installed application not found: $ApplicationPath"
}

# Execute in the existing Explorer process, not the calling IDE's process tree.
# Starting a new process directly (including Start-Process) retains that tree.
$desktopHandle = 0
$desktop = (New-Object -ComObject Shell.Application).Windows().FindWindowSW(
    0, 0, 8, [ref]$desktopHandle, 1
)
if ($null -eq $desktop -or $null -eq $desktop.Document.Application) {
    throw 'Windows desktop shell is unavailable. Launch the application from the Start menu.'
}
$desktop.Document.Application.ShellExecute(
    $ApplicationPath, '', (Split-Path -Parent $ApplicationPath), 'open', 1
)
