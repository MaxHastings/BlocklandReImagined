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
    # The default Add-Ons every release ships, as committed (packages/default-addons.json).
    $defaults = @((Get-Content (Join-Path $repo 'packages/default-addons.json') -Raw | ConvertFrom-Json).addons)
    foreach ($id in @('duplicator','duplicator-tool','vehicle_stunt_plane')) {
        if (@($defaults | Where-Object { $_.id -eq $id }).Count -ne 1) { throw "Expected $id in packages/default-addons.json." }
    }
    [IO.Directory]::CreateDirectory((Join-Path $fixture 'packages')) | Out-Null
    Copy-Item (Join-Path $repo 'packages/default-addons.json') (Join-Path $fixture 'packages/default-addons.json')
    foreach ($addOn in $defaults) {
        $destination = Join-Path $fixture "packages/$($addOn.path)"
        [IO.Directory]::CreateDirectory((Split-Path -Parent $destination)) | Out-Null
        Copy-Item -LiteralPath (Join-Path $repo "packages/$($addOn.path)") -Destination $destination -Recurse
    }
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
    $shippedList = Get-Content (Join-Path $package 'content/packages.json') -Raw | ConvertFrom-Json
    # After the base game, in the list's order, on the sides the game derives.
    $listed = @($shippedList.packages | Select-Object -Skip $fields.Count | ForEach-Object { "$($_.id)=$($_.side)@$($_.dir)" }) -join ' '
    $expected = 'duplicator=server@addons/duplicator duplicator-tool=shared@addons/duplicator-tool vehicle_stunt_plane=shared@addons/vehicle_stunt_plane'
    if ($listed -cne $expected) { throw "Expected the default Add-Ons turned on as $expected, got $listed." }
    foreach ($addOn in $defaults) {
        $source = @(Get-ChildItem -LiteralPath (Join-Path $repo "packages/$($addOn.path)") -Recurse -File).Count
        $copied = @(Get-ChildItem -LiteralPath (Join-Path $package "content/addons/$($addOn.id)") -Recurse -File).Count
        if ($copied -ne $source) { throw "Expected $source files of $($addOn.id) in the release, found $copied." }
    }
    if (-not (Test-Path (Join-Path $package 'content/addons/vehicle_stunt_plane/assets/vehicles.json'))) { throw "Expected the Stunt Plane's vehicles in the release." }
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

    # A release that turns the Stunt Plane off, or a build without it, is refused.
    $withoutPlane = Join-Path $temp 'without-plane'
    Copy-Item -LiteralPath $package -Destination $withoutPlane -Recurse
    Remove-Item -LiteralPath (Join-Path $withoutPlane 'unexpected.txt')
    $trimmed = [ordered]@{ schema_version = 1; packages = @($shippedList.packages | Where-Object { $_.id -ne 'vehicle_stunt_plane' }) }
    $trimmedPath = Join-Path $withoutPlane 'content/packages.json'
    [IO.File]::WriteAllText($trimmedPath, (ConvertTo-Json $trimmed -Depth 4))
    # Re-list it, so only the default Add-On check can refuse it.
    $manifest = Get-Content (Join-Path $withoutPlane 'MANIFEST.json') -Raw | ConvertFrom-Json
    $listed = @($manifest.files | Where-Object { $_.path -eq 'content/packages.json' })[0]
    $listed.bytes = (Get-Item $trimmedPath).Length
    $listed.sha256 = (Get-FileHash $trimmedPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $withoutPlane 'MANIFEST.json'), (ConvertTo-Json $manifest -Depth 10))
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyPackage $withoutPlane } catch { $caught = $_.Exception.Message -like '*vehicle_stunt_plane*' }
    if (-not $caught) { throw 'Verifier accepted a release without the Stunt Plane.' }
    Remove-Item -LiteralPath (Join-Path $fixture 'packages/imported/vehicle_stunt_plane') -Recurse -Force
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -ExecutablePath $exe -DestinationRoot (Join-Path $temp 'dist2') -Version 'test-fixture' -ExpectedExecutableSha256 $exeHash -SkipVersionCheck -CompanionExecutables @() } catch { $caught = $_.Exception.Message -like '*Default Add-On vehicle_stunt_plane is missing*' }
    if (-not $caught) { throw 'Packager built a release without the Stunt Plane.' }

    Write-Host 'Packaging fixture tests passed.'
} finally {
    $fullTemp = [IO.Path]::GetFullPath($temp)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($fullTemp.StartsWith($tempRoot,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $fullTemp -PathType Container)) {
        Remove-Item -LiteralPath $fullTemp -Recurse -Force
    }
}
