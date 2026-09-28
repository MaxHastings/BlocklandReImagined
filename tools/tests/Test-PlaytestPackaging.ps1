[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$temp = Join-Path ([IO.Path]::GetTempPath()) "bri-package-test-$([Guid]::NewGuid().ToString('N'))"
[IO.Directory]::CreateDirectory($temp) | Out-Null
try {
    $fixture = Join-Path $temp 'fixture'
    [IO.Directory]::CreateDirectory($fixture) | Out-Null
    foreach ($path in @('content','docs','bin')) { [IO.Directory]::CreateDirectory((Join-Path $fixture $path)) | Out-Null }
    Copy-Item (Join-Path $repo 'docs/PLAYTEST.md') (Join-Path $fixture 'docs/PLAYTEST.md')
    Copy-Item (Join-Path $repo 'docs/KNOWN-ISSUES.md') (Join-Path $fixture 'docs/KNOWN-ISSUES.md')
    Copy-Item (Join-Path $repo 'docs/TESTER-GUIDE.md') (Join-Path $fixture 'docs/TESTER-GUIDE.md')
    Copy-Item (Join-Path $repo 'docs/FEATURES.md') (Join-Path $fixture 'docs/FEATURES.md')
    $fields = @('map_bundle','brick_catalog','geometry','effects','worlds','ui_pack','brick_materials','avatar','effects_runtime','audio','weather','foliage','weapons','item_presentation','weapon_debris','vehicles','events','tutorial')
    $packages = @()
    foreach ($field in $fields) {
        $packages += [ordered]@{ id = "fixture-$($field.Replace('_','-'))"; version = '1.0.0'; side = 'shared'; dir = "fixture-$field"; role = $field }
        $packageDir = Join-Path $fixture "content/fixture-$field"
        [IO.Directory]::CreateDirectory($packageDir) | Out-Null
        [IO.File]::WriteAllText((Join-Path $packageDir 'asset.bin'), "fixture-$field")
    }
    $override = [ordered]@{ schema_version = 1; packages = $packages }
    [IO.File]::WriteAllText((Join-Path $fixture 'content/packages.json'), (ConvertTo-Json $override -Depth 4))
    $exe = Join-Path $fixture 'bin/bri-client.exe'
    [IO.File]::WriteAllBytes($exe, [byte[]](0x4d,0x5a,0x01,0x02))
    $exeHash = (Get-FileHash $exe -Algorithm SHA256).Hash
    # A stand-in for the standalone launcher: the packager only appends to it.
    [IO.File]::WriteAllBytes((Join-Path $fixture 'bin/BlocklandReImagined.exe'), [byte[]](0x4d,0x5a,0x03,0x04))
    $dist = Join-Path $temp 'dist'
    & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -ExecutablePath $exe -DestinationRoot $dist -Version 'test-fixture' -ExpectedExecutableSha256 $exeHash -SkipVersionCheck -CompanionExecutables @()
    $package = Join-Path $dist 'BlocklandReImagined-alpha-test-fixture'
    if (-not (Test-Path (Join-Path $package 'Launch.cmd'))) { throw 'Expected package launcher Launch.cmd.' }
    if (Test-Path (Join-Path $package 'Launch-Playtest.cmd')) { throw 'Unexpected old launcher filename.' }
    foreach ($doc in @('TESTER-GUIDE.md','FEATURES.md')) { if (-not (Test-Path (Join-Path $package $doc))) { throw "Expected $doc in the release folder." } }
    & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyPackage $package
    if (-not (Test-Path "$package.zip" -PathType Leaf)) { throw 'Expected the release zip beside the folder.' }
    $standalone = Join-Path "$package-standalone" 'BlocklandReImagined.exe'
    if (-not (Test-Path $standalone -PathType Leaf)) { throw 'Expected the standalone BlocklandReImagined.exe.' }
    & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyStandalone $standalone
    $bytes = [IO.File]::ReadAllBytes($standalone)
    $bytes[10] = $bytes[10] -bxor 0xff
    $damaged = Join-Path $temp 'damaged.exe'
    [IO.File]::WriteAllBytes($damaged, $bytes)
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyStandalone $damaged } catch { $caught = $true }
    if (-not $caught) { throw 'Verifier accepted a damaged standalone payload.' }
    [IO.Directory]::CreateDirectory((Join-Path $package 'logs')) | Out-Null
    [IO.Directory]::CreateDirectory((Join-Path $package 'user-state')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $package 'logs/session.log'), 'mutable')
    [IO.File]::WriteAllText((Join-Path $package 'user-state/preferences.json'), '{}')
    & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyPackage $package
    [IO.File]::WriteAllText((Join-Path $package 'unexpected.txt'), 'unlisted')
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyPackage $package } catch { $caught = $true }
    if (-not $caught) { throw 'Verifier accepted an unlisted immutable file.' }

    Write-Host 'Packaging fixture tests passed.'
} finally {
    $fullTemp = [IO.Path]::GetFullPath($temp)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($fullTemp.StartsWith($tempRoot,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $fullTemp -PathType Container)) {
        Remove-Item -LiteralPath $fullTemp -Recurse -Force
    }
}
