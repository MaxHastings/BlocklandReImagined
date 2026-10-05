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
    Copy-Item (Join-Path $repo 'docs/DEDICATED-SERVER.md') (Join-Path $fixture 'docs/DEDICATED-SERVER.md')
    Copy-Item (Join-Path $repo 'docs/rule-workshop') (Join-Path $fixture 'docs/rule-workshop') -Recurse
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
    # The default Add-Ons every release ships (packages/default-addons.json):
    # our own as committed, and a stand-in for each bundled original that
    # ships, as tools/addon_bundle.py build leaves it in the bundle.
    $defaults = @((Get-Content (Join-Path $repo 'packages/default-addons.json') -Raw | ConvertFrom-Json).addons)
    foreach ($id in @('tool_duplicator','vehicle_stunt_plane','brick_mirror')) {
        if (@($defaults | Where-Object { $_.id -eq $id }).Count -ne 1) { throw "Expected $id in packages/default-addons.json." }
    }
    [IO.Directory]::CreateDirectory((Join-Path $fixture 'packages')) | Out-Null
    Copy-Item (Join-Path $repo 'packages/default-addons.json') (Join-Path $fixture 'packages/default-addons.json')
    foreach ($entry in Get-ChildItem (Join-Path $repo 'crates/addon-import/ports/*/entry.json')) {
        $to = Join-Path $fixture ('crates/addon-import/ports/' + $entry.Directory.Name)
        [IO.Directory]::CreateDirectory($to) | Out-Null
        Copy-Item $entry.FullName (Join-Path $to 'entry.json')
    }
    $bundle = Join-Path $fixture 'dist/addon-bundle'
    $credits = @('# Bundled Add-On credits', '')
    $shipping = @()
    foreach ($addOn in $defaults) {
        $original = $addOn.PSObject.Properties['original']
        if ($null -eq $original) {
            $destination = Join-Path $fixture "packages/$($addOn.path)"
            [IO.Directory]::CreateDirectory((Split-Path -Parent $destination)) | Out-Null
            Copy-Item -LiteralPath (Join-Path $repo "packages/$($addOn.path)") -Destination $destination -Recurse
            $shipping += $addOn
            continue
        }
        $original = $original.Value
        if (@($original.sha256).Count -eq 0 -or $null -ne $original.PSObject.Properties['withdrawn']) { continue }
        $dir = Join-Path $bundle "addons/$($addOn.id)"
        [IO.Directory]::CreateDirectory((Join-Path $dir 'assets')) | Out-Null
        [IO.File]::WriteAllText((Join-Path $dir 'assets/vehicles.json'), '{ "schema_version": 1, "definitions": [] }')
        $manifest = [ordered]@{ schema_version = 1; id = $addOn.id; version = $original.version; api = 1; name = $original.title
            authors = @($original.authors); provenance = [ordered]@{ source = "Blockland Add-On $($original.addon) (zip), sha256 $(@($original.sha256)[0])"; bundled = 'stand-in' }
            provides = @([ordered]@{ kind = 'vehicles'; id = "$($addOn.id):vehicles/main"; file = 'assets/vehicles.json' }) }
        # The Duplicator's port writes host rules: Import leaves them beside
        # it at addons/<id>-rules, named in its companions.
        if ($addOn.id -eq 'tool_duplicator') {
            $manifest['companions'] = @("$($addOn.id)-rules")
            $rules = Join-Path $bundle "addons/$($addOn.id)-rules"
            [IO.Directory]::CreateDirectory($rules) | Out-Null
            [IO.File]::WriteAllText((Join-Path $rules 'behaviour.json'), '{}')
            $rulesManifest = [ordered]@{ schema_version = 1; id = "$($addOn.id)-rules"; version = $original.version; api = 1
                dependencies = [ordered]@{ $addOn.id = "=$($original.version)" }; capabilities = @('player')
                provides = @([ordered]@{ kind = 'behaviour'; id = "$($addOn.id)-rules:behaviour/behaviour"; file = 'behaviour.json' }) }
            [IO.File]::WriteAllText((Join-Path $rules 'package.json'), (ConvertTo-Json $rulesManifest -Depth 5))
        }
        [IO.File]::WriteAllText((Join-Path $dir 'package.json'), (ConvertTo-Json $manifest -Depth 5))
        $credits += "- **$($original.title)** by $(@($original.authors) -join ', ')"
        $shipping += $addOn
    }
    [IO.File]::WriteAllText((Join-Path $bundle 'CREDITS.md'), ($credits -join "`n") + "`n")
    $exe = Join-Path $fixture 'bin/bri-client.exe'
    [IO.File]::WriteAllBytes($exe, [byte[]](0x4d,0x5a,0x01,0x02))
    $exeHash = (Get-FileHash $exe -Algorithm SHA256).Hash
    # A stand-in DirectX Shader Compiler release and its pin
    # (tools/shader-compiler.json): the packager takes the pinned files out
    # of the verified archive and puts them beside the game.
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $dxcFiles = [ordered]@{ 'bin/x64/dxcompiler.dll' = 'dxcompiler.dll'; 'LICENSE-LLVM.txt' = 'licenses/DirectXShaderCompiler/LICENSE-LLVM.txt' }
    $dxcSource = Join-Path $temp 'dxc-source'
    $ships = @()
    foreach ($from in $dxcFiles.Keys) {
        $path = Join-Path $dxcSource $from
        [IO.Directory]::CreateDirectory((Split-Path -Parent $path)) | Out-Null
        [IO.File]::WriteAllText($path, "stand-in $from")
        $ships += [ordered]@{ from = $from; to = $dxcFiles[$from]; sha256 = (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant() }
    }
    $dxcArchive = Join-Path $temp 'dxc-standin.zip'
    # Entry names with forward slashes, as the real release's.
    $writing = [IO.Compression.ZipFile]::Open($dxcArchive, [IO.Compression.ZipArchiveMode]::Create)
    foreach ($from in $dxcFiles.Keys) {
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($writing, (Join-Path $dxcSource $from), $from) | Out-Null
    }
    $writing.Dispose()
    $pin = [ordered]@{ schema_version = 1; name = 'DirectX Shader Compiler'; version = 'stand-in'; url = 'https://example.invalid/dxc.zip'
        sha256 = (Get-FileHash $dxcArchive -Algorithm SHA256).Hash.ToLowerInvariant(); ships = $ships }
    [IO.Directory]::CreateDirectory((Join-Path $fixture 'tools')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixture 'tools/shader-compiler.json'), (ConvertTo-Json $pin -Depth 4))
    # An archive that is not the pinned one is refused.
    $tampered = Join-Path $temp 'dxc-tampered.zip'
    Copy-Item $dxcArchive $tampered
    [IO.File]::AppendAllText($tampered, 'x')
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -ExecutablePath $exe -DestinationRoot (Join-Path $temp 'dist-tampered') -Version 'test-fixture' -ExpectedExecutableSha256 $exeHash -SkipVersionCheck -CompanionExecutables @() -ShaderCompilerArchive $tampered } catch { $caught = $_.Exception.Message }
    if ($caught -notlike '*not the pinned*') { throw "Packager accepted a shader compiler archive that is not the pinned one: $caught" }
    $dist = Join-Path $temp 'dist'
    & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -ExecutablePath $exe -DestinationRoot $dist -Version 'test-fixture' -ExpectedExecutableSha256 $exeHash -SkipVersionCheck -CompanionExecutables @() -ShaderCompilerArchive $dxcArchive
    $package = Join-Path $dist 'BlocklandReImagined-test-fixture-windows'
    if (-not (Test-Path (Join-Path $package 'Launch.cmd'))) { throw 'Expected package launcher Launch.cmd.' }
    if (Test-Path (Join-Path $package 'Launch-Playtest.cmd')) { throw 'Unexpected old launcher filename.' }
    foreach ($doc in @('TESTER-GUIDE.md','FEATURES.md')) { if (-not (Test-Path (Join-Path $package $doc))) { throw "Expected $doc in the release folder." } }
    & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -VerifyPackage $package
    if (-not (Select-String -LiteralPath (Join-Path $package 'CREDITS.md') -Pattern 'Kaje, Ephialtes' -SimpleMatch -Quiet)) { throw 'Expected the Stunt Plane credited in CREDITS.md.' }
    $shippedList = Get-Content (Join-Path $package 'content/packages.json') -Raw | ConvertFrom-Json
    # After the base game, in the list's order, on the sides the game derives.
    $listed = @($shippedList.packages | Select-Object -Skip $fields.Count | ForEach-Object { "$($_.id)=$($_.side)@$($_.dir)" }) -join ' '
    # The Duplicator's host rules right after it, host-only; the Rule
    # Workshop toys are rules only, so host-only too.
    $expected = @($shipping | Where-Object { $null -eq $_.PSObject.Properties['enabled'] -or $_.enabled } | ForEach-Object {
        $side = if ($_.id -eq 'rule-workshop-toys') { 'server' } else { 'shared' }
        "$($_.id)=$side@addons/$($_.id)"
        if ($_.id -eq 'tool_duplicator') { "tool_duplicator-rules=server@addons/tool_duplicator-rules" } }) -join ' '
    if ($listed -cne $expected) { throw "Expected the default Add-Ons turned on as $expected, got $listed." }
    foreach ($addOn in $shipping) {
        $source = if ($null -ne $addOn.PSObject.Properties['path']) { Join-Path $repo "packages/$($addOn.path)" } else { Join-Path $bundle "addons/$($addOn.id)" }
        $copied = @(Get-ChildItem -LiteralPath (Join-Path $package "content/addons/$($addOn.id)") -Recurse -File).Count
        if ($copied -ne @(Get-ChildItem -LiteralPath $source -Recurse -File).Count) { throw "Expected every file of $($addOn.id) in the release." }
    }
    if (-not (Test-Path (Join-Path $package 'content/addons/tool_duplicator-rules/behaviour.json'))) { throw "Expected the Duplicator's host rules in the release." }
    # The shader compiler sits beside the game, with its licence.
    foreach ($to in $dxcFiles.Values) { if (-not (Test-Path (Join-Path $package $to) -PathType Leaf)) { throw "Expected $to in the release." } }
    # The download keeps one name across versions and holds the versioned folder.
    $zip = Join-Path $dist 'BlocklandReImagined-windows.zip'
    if (-not (Test-Path $zip -PathType Leaf)) { throw 'Expected BlocklandReImagined-windows.zip beside the folder.' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $unzipped = Join-Path $temp 'unzipped'
    [IO.Compression.ZipFile]::ExtractToDirectory($zip, $unzipped)
    $tops = @(Get-ChildItem -LiteralPath $unzipped -Force)
    if ($tops.Count -ne 1 -or $tops[0].Name -cne 'BlocklandReImagined-test-fixture-windows') { throw 'Expected the zip to hold exactly the release folder.' }
    & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -VerifyPackage $tops[0].FullName
    [IO.Directory]::CreateDirectory((Join-Path $package 'logs')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $package 'logs/session.log'), 'mutable')
    & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -VerifyPackage $package
    [IO.File]::WriteAllText((Join-Path $package 'unexpected.txt'), 'unlisted')
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -VerifyPackage $package } catch { $caught = $true }
    if (-not $caught) { throw 'Verifier accepted an unlisted immutable file.' }

    # A release without the shader compiler, manifest and all, is refused.
    $withoutDxc = Join-Path $temp 'without-dxc'
    Copy-Item -LiteralPath $package -Destination $withoutDxc -Recurse
    Remove-Item -LiteralPath (Join-Path $withoutDxc 'unexpected.txt')
    Remove-Item -LiteralPath (Join-Path $withoutDxc 'dxcompiler.dll')
    $manifest = Get-Content (Join-Path $withoutDxc 'MANIFEST.json') -Raw | ConvertFrom-Json
    $manifest.files = @($manifest.files | Where-Object { $_.path -cne 'dxcompiler.dll' })
    [IO.File]::WriteAllText((Join-Path $withoutDxc 'MANIFEST.json'), (ConvertTo-Json $manifest -Depth 10))
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -VerifyPackage $withoutDxc } catch { $caught = $_.Exception.Message -like '*dxcompiler.dll*' }
    if (-not $caught) { throw 'Verifier accepted a release without the shader compiler.' }

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
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -VerifyPackage $withoutPlane } catch { $caught = $_.Exception.Message -like '*vehicle_stunt_plane*' }
    if (-not $caught) { throw 'Verifier accepted a release without the Stunt Plane.' }
    Remove-Item -LiteralPath (Join-Path $bundle 'addons/vehicle_stunt_plane') -Recurse -Force
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -ExecutablePath $exe -DestinationRoot (Join-Path $temp 'dist2') -Version 'test-fixture' -ExpectedExecutableSha256 $exeHash -SkipVersionCheck -CompanionExecutables @() -ShaderCompilerArchive $dxcArchive } catch { $caught = $_.Exception.Message }
    if ($caught -notlike '*lacks Vehicle_Stunt_Plane (vehicle_stunt_plane)*') { throw "Packager built a release without the Stunt Plane: $caught" }

    Write-Host 'Packaging fixture tests passed.'
} finally {
    $fullTemp = [IO.Path]::GetFullPath($temp)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($fullTemp.StartsWith($tempRoot,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $fullTemp -PathType Container)) {
        Remove-Item -LiteralPath $fullTemp -Recurse -Force
    }
}
