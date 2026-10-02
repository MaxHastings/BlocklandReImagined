$ErrorActionPreference = 'Stop'
$packageRoot = [IO.Path]::GetFullPath($PSScriptRoot)
Set-Location -LiteralPath $packageRoot
$logDirectory = Join-Path $packageRoot 'logs'
[IO.Directory]::CreateDirectory($logDirectory) | Out-Null
$stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff', [Globalization.CultureInfo]::InvariantCulture)
$stdout = Join-Path $logDirectory "client-$stamp.stdout.log"
$stderr = Join-Path $logDirectory "client-$stamp.stderr.log"
$executable = Join-Path $packageRoot 'bri-client.exe'
try {
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw "Missing packaged client: $executable" }
    if (-not (Test-Path -LiteralPath (Join-Path $packageRoot 'content/packages.json') -PathType Leaf)) { throw 'The package has no content/packages.json.' }
    # Windows PowerShell turns redirected native stderr into ErrorRecords. With
    # Stop that swallowed the client's first error and left an empty error log.
    # Let the OS redirect streams directly and preserve the real process status.
    # No state folder argument: settings and saves go to the game's per-user
    # folder (%LOCALAPPDATA%\BlocklandReImagined), so a newer release's
    # folder finds them.
    $clientProcess = Start-Process -FilePath $executable -ArgumentList @('--run', '.\content') -NoNewWindow -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $exitCode = $clientProcess.ExitCode
    Write-Host "Client exited with code $exitCode. Logs: $stdout and $stderr"
    if ($exitCode -ne 0) { Write-Host 'Check the stderr log for startup or runtime errors.' }
    exit $exitCode
} catch {
    Write-Host "Could not launch the playtest client: $($_.Exception.Message)"
    Write-Host "Logs, if the client started: $stdout and $stderr"
    exit 1
}
