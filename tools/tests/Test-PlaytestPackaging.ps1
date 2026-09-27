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
    foreach ($path in @('crates/client/src','content','docs','bin')) { [IO.Directory]::CreateDirectory((Join-Path $fixture $path)) | Out-Null }
    Copy-Item (Join-Path $repo 'crates/client/src/content.rs') (Join-Path $fixture 'crates/client/src/content.rs')
    Copy-Item (Join-Path $repo 'docs/PLAYTEST.md') (Join-Path $fixture 'docs/PLAYTEST.md')
    Copy-Item (Join-Path $repo 'docs/KNOWN-ISSUES.md') (Join-Path $fixture 'docs/KNOWN-ISSUES.md')
    $fields = @('map_bundle','brick_catalog','geometry','effects','worlds','ui_pack','brick_materials','avatar','effects_runtime','audio','weather','foliage','weapons','item_presentation')
    $override = [ordered]@{ schema_version = 1 }
    foreach ($field in $fields) {
        $override[$field] = "fixture-$field"
        $packageDir = Join-Path $fixture "content/fixture-$field"
        [IO.Directory]::CreateDirectory($packageDir) | Out-Null
        [IO.File]::WriteAllText((Join-Path $packageDir 'asset.bin'), "fixture-$field")
    }
    $override.terrain_region = @(0,0,1,1)
    [IO.File]::WriteAllText((Join-Path $fixture 'content/client-content.json'), (ConvertTo-Json $override -Depth 4))
    $exe = Join-Path $fixture 'bin/bri-client.exe'
    [IO.File]::WriteAllBytes($exe, [byte[]](0x4d,0x5a,0x01,0x02))
    $exeHash = (Get-FileHash $exe -Algorithm SHA256).Hash
    $dist = Join-Path $temp 'dist'
    & (Join-Path $repo 'tools/package_playtest.ps1') -RepoRoot $fixture -ExecutablePath $exe -DestinationRoot $dist -Version 'test-fixture' -ExpectedExecutableSha256 $exeHash
    $package = Join-Path $dist 'BlocklandReImagined-building-playtest-test-fixture'
    if (-not (Test-Path (Join-Path $package 'Launch.cmd'))) { throw 'Expected package launcher Launch.cmd.' }
    if (Test-Path (Join-Path $package 'Launch-Playtest.cmd')) { throw 'Unexpected old launcher filename.' }
    & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyPackage $package
    [IO.Directory]::CreateDirectory((Join-Path $package 'logs')) | Out-Null
    [IO.Directory]::CreateDirectory((Join-Path $package 'user-state')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $package 'logs/session.log'), 'mutable')
    [IO.File]::WriteAllText((Join-Path $package 'user-state/preferences.json'), '{}')
    & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyPackage $package
    [IO.File]::WriteAllText((Join-Path $package 'unexpected.txt'), 'unlisted')
    $caught = $false
    try { & (Join-Path $repo 'tools/package_playtest.ps1') -VerifyPackage $package } catch { $caught = $true }
    if (-not $caught) { throw 'Verifier accepted an unlisted immutable file.' }

    $state = Join-Path $temp 'host-state'
    $certDir = Join-Path $temp 'certs'
    [IO.Directory]::CreateDirectory($certDir) | Out-Null
    function New-TestCertificate([string]$Path) {
        $request = [Security.Cryptography.X509Certificates.CertificateRequest]::new('CN=fixture-host', [Security.Cryptography.RSA]::Create(2048), [Security.Cryptography.HashAlgorithmName]::SHA256, [Security.Cryptography.RSASignaturePadding]::Pkcs1)
        $cert = $request.CreateSelfSigned([DateTimeOffset]::UtcNow.AddDays(-1), [DateTimeOffset]::UtcNow.AddDays(7))
        [IO.File]::WriteAllBytes($Path, $cert.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert))
        $cert.Dispose()
    }
    $certOne = Join-Path $certDir 'one.der'; $certTwo = Join-Path $certDir 'two.der'
    New-TestCertificate $certOne; New-TestCertificate $certTwo
    $trust = Join-Path $repo 'tools/Trust-Host.ps1'
    & $trust -NonInteractive -Address '127.0.0.1:28000' -CertificatePath $certOne -StateDirectory $state | Out-Null
    $pinPath = Join-Path $state 'trusted-hosts.json'
    $pins = Get-Content $pinPath -Raw | ConvertFrom-Json
    if ($pins.'127.0.0.1:28000'.Count -ne (Get-Item $certOne).Length) { throw 'Saved DER pin bytes did not match input.' }
    & $trust -NonInteractive -Address '127.0.0.1:28000' -CertificatePath $certOne -StateDirectory $state | Out-Null
    $caught = $false
    try { & $trust -NonInteractive -Address '127.0.0.1:28000' -CertificatePath $certTwo -StateDirectory $state | Out-Null } catch { $caught = $true }
    if (-not $caught) { throw 'Trust helper replaced a changed pin without authorization.' }
    & $trust -NonInteractive -ReplaceExisting -Address '127.0.0.1:28000' -CertificatePath $certTwo -StateDirectory $state | Out-Null
    $pins = Get-Content $pinPath -Raw | ConvertFrom-Json
    if ($pins.'127.0.0.1:28000'.Count -ne (Get-Item $certTwo).Length) { throw 'Explicit pin replacement was not saved.' }
    Write-Host 'Packaging and trust-helper fixture tests passed.'
} finally {
    $fullTemp = [IO.Path]::GetFullPath($temp)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($fullTemp.StartsWith($tempRoot,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $fullTemp -PathType Container)) {
        Remove-Item -LiteralPath $fullTemp -Recurse -Force
    }
}
