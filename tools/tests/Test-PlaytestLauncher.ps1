[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# A tiny console fixture exercises Windows PowerShell's real native-process
# behavior. Never starts the game, a window, an audio device, or desktop input.
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('bri launcher fixture ' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory((Join-Path $fixture 'content')) | Out-Null
try {
    Copy-Item -LiteralPath (Join-Path $repo 'tools/Launch-Playtest.ps1') -Destination $fixture
    [IO.File]::WriteAllText((Join-Path $fixture 'content/packages.json'), '{}')
    $source = @'
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args, ["--run", ".\\content", ".\\user-state"]);
    assert!(std::path::Path::new("user-state").is_dir());
    eprintln!("fixture diagnostic on stderr");
    println!("fixture normal output");
    let exit = std::fs::read_to_string("exit-code.txt").unwrap().parse::<i32>().unwrap();
    std::process::exit(exit);
}
'@
    [IO.File]::WriteAllText((Join-Path $fixture 'fixture.rs'), $source)
    & rustc --crate-name launcher_fixture (Join-Path $fixture 'fixture.rs') -o (Join-Path $fixture 'bri-client.exe')
    if ($LASTEXITCODE -ne 0) { throw 'Failed to compile launcher fixture.' }
    foreach ($exitCode in @(0,7)) {
        [IO.File]::WriteAllText((Join-Path $fixture 'exit-code.txt'), [string]$exitCode)
        & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File (Join-Path $fixture 'Launch-Playtest.ps1')
        if ($LASTEXITCODE -ne $exitCode) { throw "Launcher did not preserve native exit $exitCode." }
        $errorLog = Get-ChildItem -LiteralPath (Join-Path $fixture 'logs') -Filter '*.stderr.log' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        $outLog = [IO.Path]::Combine($errorLog.DirectoryName, $errorLog.Name.Replace('.stderr.log','.stdout.log'))
        if ((Get-Content -LiteralPath $errorLog.FullName -Raw) -notmatch 'fixture diagnostic on stderr') { throw 'Native stderr missing from error log.' }
        if ((Get-Content -LiteralPath $outLog -Raw) -notmatch 'fixture normal output') { throw 'Native stdout missing from output log.' }
    }
    Write-Host 'Launcher captures both streams and preserves success/failure exit codes under Windows PowerShell.'
} finally {
    $resolvedFixture = [IO.Path]::GetFullPath($fixture)
    $tempPrefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($resolvedFixture.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolvedFixture -PathType Container)) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}
