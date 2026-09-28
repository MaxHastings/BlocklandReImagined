[CmdletBinding()]
param(
    [string]$RepoRoot = (Split-Path -Parent $PSScriptRoot),
    [string]$ExecutablePath,
    [string]$DestinationRoot,
    [string]$Version,
    [string]$ExpectedExecutableSha256,
    [switch]$ValidateOnly,
    [string]$VerifyPackage,
    # Also ship the Stress Lab mod packages (packages/stresslab), enabled in
    # content/packages.json; the release folder gets a -stress-lab suffix.
    [switch]$StressLab,
    # Tools the client runs, shipped beside bri-client.exe from the same build.
    [string[]]$CompanionExecutables = @('bri-import-addon.exe'),
    # Code signing, for when there is a certificate: the SHA-1 thumbprint of
    # a code-signing certificate in the current user's or machine's store.
    # Every shipped .exe is signed and timestamped, which stops Windows
    # SmartScreen's "Windows protected your PC" once the certificate has
    # reputation. Without it the package is unsigned (the README explains
    # "Run anyway").
    [string]$SignCertificateThumbprint,
    [string]$TimestampUrl = 'http://timestamp.digicert.com',
    # Packaging tests use a stand-in executable that cannot report a version.
    [switch]$SkipVersionCheck
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path
if ([string]::IsNullOrWhiteSpace($ExecutablePath)) { $ExecutablePath = Join-Path $RepoRoot 'target/release/bri-client.exe' }
if ([string]::IsNullOrWhiteSpace($DestinationRoot)) { $DestinationRoot = Join-Path $RepoRoot 'dist' }
$ExecutablePath = [IO.Path]::GetFullPath($ExecutablePath)
$DestinationRoot = [IO.Path]::GetFullPath($DestinationRoot)
$script:PackFields = @('map_bundle','brick_catalog','geometry','effects','worlds','ui_pack','brick_materials','avatar','effects_runtime','audio','weather','foliage','weapons','item_presentation','weapon_debris','vehicles','events','tutorial')

function Get-PackageFiles([string]$Path) {
    $all = @(Get-ChildItem -LiteralPath $Path -Force -Recurse)
    foreach ($item in $all) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Package inputs may not contain symbolic links or junctions: $($item.FullName)" }
    }
    return @($all | Where-Object { -not $_.PSIsContainer })
}

function Read-PackageList([string]$Path) {
    $info = Get-Item -LiteralPath $Path
    if (($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "$Path must not be a symbolic link." }
    if ($info.Length -gt 1048576) { throw "$Path exceeds 1 MiB." }
    $list = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if ([int]$list.schema_version -ne 1) { throw "Unsupported package list schema version in $Path." }
    return $list
}

# The packages the client loads: content/packages.json when present,
# otherwise the base game's list (crates/package/base-packages.json).
function Get-EffectivePackages([string]$Root) {
    $override = Join-Path (Join-Path $Root 'content') 'packages.json'
    if (Test-Path -LiteralPath $override -PathType Leaf) {
        $list = Read-PackageList $override; $source = 'content/packages.json override'
    } else {
        $base = Join-Path $Root 'crates/package/base-packages.json'
        if (-not (Test-Path -LiteralPath $base -PathType Leaf)) { throw "Missing base package list: $base" }
        $list = Read-PackageList $base; $source = 'crates/package/base-packages.json'
    }
    $roles = @{}
    foreach ($package in @($list.packages)) {
        $name = [string]$package.dir
        if ([string]::IsNullOrWhiteSpace($name) -or $name.Contains('\') -or $name.Contains(':') -or
            @($name.Split('/') | Where-Object { $_ -in @('','.', '..') -or $_.EndsWith(' ') -or $_.EndsWith('.') }).Count -gt 0) {
            throw "Unsafe/empty package directory for '$($package.id)': $name"
        }
        $role = $package.PSObject.Properties['role']
        if ($null -ne $role) { $roles[[string]$role.Value] = $true }
    }
    foreach ($field in $script:PackFields) {
        if (-not $roles.ContainsKey($field)) { throw "The package list has no package for the '$field' role." }
    }
    return [pscustomobject]@{ list = $list; source = $source }
}

function Get-RelativePackagePath([string]$Root,[string]$File) {
    return $File.Substring($Root.Length).TrimStart([IO.Path]::DirectorySeparatorChar,[IO.Path]::AltDirectorySeparatorChar).Replace('\','/')
}

function Get-ManifestEntries([string]$Root) {
    $files = @(Get-PackageFiles $Root | Where-Object {
        $relative = Get-RelativePackagePath $Root $_.FullName
        -not ($relative.StartsWith('logs/',[StringComparison]::Ordinal) -or $relative.StartsWith('user-state/',[StringComparison]::Ordinal))
    })
    $paths = @($files | ForEach-Object { Get-RelativePackagePath $Root $_.FullName })
    [Array]::Sort($paths, [StringComparer]::Ordinal)
    $records = foreach ($relative in $paths) {
        $path = Join-Path $Root ($relative.Replace('/',[IO.Path]::DirectorySeparatorChar))
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        [ordered]@{ path = $relative; bytes = (Get-Item -LiteralPath $path).Length; sha256 = $hash }
    }
    return @($records)
}

function Verify-PlaytestPackage([string]$Path) {
    $root = (Resolve-Path -LiteralPath $Path).Path
    $manifestPath = Join-Path $root 'MANIFEST.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "Missing package manifest: $manifestPath" }
    $manifestInfo = Get-Item -LiteralPath $manifestPath
    if ($manifestInfo.Length -gt 16MB) { throw 'Package manifest exceeds 16 MiB.' }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ([int]$manifest.schema_version -ne 1 -or @($manifest.files).Count -eq 0) { throw 'Invalid package manifest.' }
    $listed = @{}
    $last = $null
    foreach ($entry in $manifest.files) {
        $relative = [string]$entry.path
        if ([string]::IsNullOrWhiteSpace($relative) -or $relative.Contains('\') -or $relative.Contains(':') -or
            @($relative.Split('/') | Where-Object { $_ -in @('','.', '..') }).Count -gt 0) { throw "Unsafe manifest path: $relative" }
        if ($null -ne $last -and [StringComparer]::Ordinal.Compare($last,$relative) -ge 0) { throw 'Manifest file entries are not uniquely sorted ordinally.' }
        $last = $relative
        $file = Join-Path $root ($relative.Replace('/',[IO.Path]::DirectorySeparatorChar))
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing package file: $relative" }
        $info = Get-Item -LiteralPath $file
        if (($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $info.Length -ne [long]$entry.bytes) { throw "Package size/link mismatch: $relative" }
        $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -cne [string]$entry.sha256) { throw "Package checksum mismatch: $relative" }
        $listed[$relative] = $true
    }
    $actual = @(Get-PackageFiles $root | Where-Object { $_.FullName -ne $manifestPath } | ForEach-Object { Get-RelativePackagePath $root $_.FullName } | Where-Object {
        -not ($_.StartsWith('logs/',[StringComparison]::Ordinal) -or $_.StartsWith('user-state/',[StringComparison]::Ordinal))
    })
    if ($actual.Count -ne $listed.Count) { throw "Package contains unlisted or missing files (listed $($listed.Count), found $($actual.Count))." }
    foreach ($relative in $actual) { if (-not $listed.ContainsKey($relative)) { throw "Unlisted package file: $relative" } }
    Write-Host "Verified $($listed.Count) files for package version $($manifest.version)."
}

if (-not [string]::IsNullOrWhiteSpace($VerifyPackage)) {
    Verify-PlaytestPackage $VerifyPackage
    return
}

if (-not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) { throw "Release executable is missing; root must build it first: $ExecutablePath" }
$exeInfo = Get-Item -LiteralPath $ExecutablePath
if (($exeInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $exeInfo.Length -lt 1) { throw 'Release executable is empty or linked.' }
$executableSha256 = (Get-FileHash -LiteralPath $ExecutablePath -Algorithm SHA256).Hash.ToLowerInvariant()
$companions = @(foreach ($name in $CompanionExecutables) {
    if ($name -notmatch '^[A-Za-z0-9._-]+\.exe$') { throw "Companion executable must be a bare .exe name: $name" }
    $path = Join-Path (Split-Path -Parent $ExecutablePath) $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Companion executable is missing; build it with the client: $path" }
    $info = Get-Item -LiteralPath $path
    if (($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $info.Length -lt 1) { throw "Companion executable is empty or linked: $path" }
    [pscustomobject]@{ name = $name; path = $path; bytes = $info.Length }
})
function Find-SignTool {
    $onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($null -ne $onPath) { return $onPath.Source }
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $found = @(Get-ChildItem -LiteralPath $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending)
    if ($found.Count -eq 0) { throw 'signtool.exe was not found; install the Windows SDK (Signing Tools) or put signtool on PATH.' }
    return $found[0].FullName
}

function Invoke-CodeSigning([string]$Folder) {
    $signtool = Find-SignTool
    foreach ($exe in @(Get-ChildItem -LiteralPath $Folder -Filter *.exe -File)) {
        & $signtool sign /sha1 $SignCertificateThumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 $exe.FullName
        if ($LASTEXITCODE -ne 0) { throw "Signing failed for $($exe.Name)." }
        & $signtool verify /pa $exe.FullName
        if ($LASTEXITCODE -ne 0) { throw "Signature did not verify for $($exe.Name)." }
    }
}

# The version the build carries (bri-client --version: "<name> (<hash>)").
# It must be the package's version, or the main menu, logs and update check
# would name another build. Build releases with $env:BRI_VERSION set.
function Get-BuildVersion([string]$Executable) {
    $out = [IO.Path]::GetTempFileName()
    try {
        $process = Start-Process -FilePath $Executable -ArgumentList '--version' -NoNewWindow -Wait -PassThru -RedirectStandardOutput $out
        if ($process.ExitCode -ne 0) { throw "$Executable --version failed." }
        return ((Get-Content -LiteralPath $out -Raw) -split '\s+')[0]
    } finally {
        Remove-Item -LiteralPath $out -ErrorAction SilentlyContinue
    }
}

$effective = Get-EffectivePackages $RepoRoot
$sourceContent = [IO.Path]::GetFullPath((Join-Path $RepoRoot 'content'))
$selected = @()
$contentBytes = 0L
$contentFiles = 0
foreach ($package in @($effective.list.packages)) {
    $field = [string]$package.id
    $name = [string]$package.dir
    $directory = Join-Path $sourceContent $name
    if (-not (Test-Path -LiteralPath $directory -PathType Container)) { throw "Selected $field package is missing: $directory" }
    $directoryInfo = Get-Item -LiteralPath $directory
    if (($directoryInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Selected package may not be a link: $directory" }
    $resolvedDirectory = (Resolve-Path -LiteralPath $directory).Path
    $contentPrefix = $sourceContent.TrimEnd([IO.Path]::DirectorySeparatorChar,[IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedDirectory.StartsWith($contentPrefix,[StringComparison]::OrdinalIgnoreCase)) { throw "Selected package escapes content root: $name" }
    $files = @(Get-PackageFiles $directory)
    if ($files.Count -eq 0) { throw "Selected $field package is empty: $directory" }
    $bytes = ($files | Measure-Object -Property Length -Sum).Sum
    $contentBytes += [long]$bytes; $contentFiles += $files.Count
    $selected += [pscustomobject]@{ field = $field; name = $name; path = $directory; files = $files.Count; bytes = [long]$bytes }
}

$modPackages = @()
if ($StressLab) {
    $modRoot = Join-Path $RepoRoot 'packages/stresslab'
    foreach ($dir in @(Get-ChildItem -LiteralPath $modRoot -Directory | Sort-Object Name)) {
        $manifestPath = Join-Path $dir.FullName 'package.json'
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { continue }
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
        $files = @(Get-PackageFiles $dir.FullName)
        $side = if (@($manifest.provides | Where-Object { $_.kind -in @('behaviour','script','world','entity') }).Count -gt 0) { 'server' } else { 'client' }
        $modPackages += [pscustomobject]@{ id = [string]$manifest.id; version = [string]$manifest.version; side = $side; path = $dir.FullName; files = $files.Count }
    }
    if ($modPackages.Count -eq 0) { throw "No Stress Lab packages found in $modRoot" }
}

$docInputs = @(
    @{ source = (Join-Path $RepoRoot 'docs/PLAYTEST.md'); destination = 'PLAYTEST.md' },
    @{ source = (Join-Path $RepoRoot 'docs/KNOWN-ISSUES.md'); destination = 'KNOWN-ISSUES.md' },
    @{ source = (Join-Path $PSScriptRoot 'Launch-Playtest.ps1'); destination = 'Launch-Playtest.ps1' },
    @{ source = (Join-Path $PSScriptRoot 'Launch-Playtest.cmd'); destination = 'Launch.cmd' }
)
if ($StressLab) { $docInputs += @{ source = (Join-Path $RepoRoot 'docs/stress-lab/PLAYTEST-STRESS-LAB.md'); destination = 'PLAYTEST-STRESS-LAB.md' } }
foreach ($input in $docInputs) { if (-not (Test-Path -LiteralPath $input.source -PathType Leaf)) { throw "Required package file is missing: $($input.source)" } }
if ($ValidateOnly) {
    $configSource = $effective.source
    [pscustomobject]@{ selected_packages = $selected; content_files = $contentFiles; content_bytes = $contentBytes;
        executable_bytes = $exeInfo.Length; estimated_package_bytes = [long]$contentBytes + [long]$exeInfo.Length;
        executable_sha256 = $executableSha256; package_count = $selected.Count; content_config_source = $configSource; mod_packages = $modPackages; companion_executables = $companions } | ConvertTo-Json -Depth 6
    return
}
if ([string]::IsNullOrWhiteSpace($Version) -or $Version -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') { throw 'Supply -Version using 1–64 letters, digits, dot, underscore or dash.' }
if ([string]::IsNullOrWhiteSpace($ExpectedExecutableSha256) -or $ExpectedExecutableSha256 -notmatch '^[0-9A-Fa-f]{64}$') { throw 'Supply the SHA-256 reported for the root-provided release executable using -ExpectedExecutableSha256.' }
if ($executableSha256 -cne $ExpectedExecutableSha256.ToLowerInvariant()) { throw "Release executable hash differs from root's expected build: $executableSha256" }
$buildVersion = if ($SkipVersionCheck) { $Version } else { Get-BuildVersion $ExecutablePath }
if ($buildVersion -cne $Version) { throw "The executable reports version '$buildVersion', not '$Version'. Rebuild with `$env:BRI_VERSION = '$Version' before cargo build --release." }
if (-not [string]::IsNullOrWhiteSpace($SignCertificateThumbprint) -and $SignCertificateThumbprint -notmatch '^[0-9A-Fa-f]{40}$') { throw 'Supply -SignCertificateThumbprint as the 40-hex-digit SHA-1 thumbprint.' }
[IO.Directory]::CreateDirectory($DestinationRoot) | Out-Null
$suffix = if ($StressLab) { '-stress-lab' } else { '' }
$releasePath = Join-Path $DestinationRoot "BlocklandReImagined-alpha-$Version$suffix"
if (Test-Path -LiteralPath $releasePath) { throw "Refusing to overwrite an existing playtest release: $releasePath" }
[IO.Directory]::CreateDirectory($releasePath) | Out-Null
try {
    Copy-Item -LiteralPath $ExecutablePath -Destination (Join-Path $releasePath 'bri-client.exe')
    foreach ($companion in $companions) { Copy-Item -LiteralPath $companion.path -Destination (Join-Path $releasePath $companion.name) }
    if (-not [string]::IsNullOrWhiteSpace($SignCertificateThumbprint)) { Invoke-CodeSigning $releasePath }
    foreach ($packageInput in $docInputs) { Copy-Item -LiteralPath $packageInput.source -Destination (Join-Path $releasePath $packageInput.destination) }
    $packagedContent = Join-Path $releasePath 'content'
    [IO.Directory]::CreateDirectory($packagedContent) | Out-Null
    foreach ($package in $selected) {
        $destination = Join-Path $packagedContent $package.name
        [IO.Directory]::CreateDirectory($destination) | Out-Null
        foreach ($child in Get-ChildItem -LiteralPath $package.path -Force) {
            Copy-Item -LiteralPath $child.FullName -Destination (Join-Path $destination $child.Name) -Recurse
        }
    }
    $list = [ordered]@{ schema_version = $effective.list.schema_version; packages = @($effective.list.packages) }
    foreach ($mod in $modPackages) {
        $destination = Join-Path $packagedContent (Join-Path 'stresslab' $mod.id)
        [IO.Directory]::CreateDirectory($destination) | Out-Null
        foreach ($child in Get-ChildItem -LiteralPath $mod.path -Force) {
            Copy-Item -LiteralPath $child.FullName -Destination (Join-Path $destination $child.Name) -Recurse
        }
        $list.packages += [pscustomobject][ordered]@{ id = $mod.id; version = $mod.version; side = $mod.side; dir = "stresslab/$($mod.id)" }
    }
    $configJson = ConvertTo-Json -InputObject $list -Depth 5
    [IO.File]::WriteAllText((Join-Path $packagedContent 'packages.json'), $configJson + "`n", [Text.UTF8Encoding]::new($false))
    $entries = Get-ManifestEntries $releasePath
    $manifest = [ordered]@{ schema_version = 1; version = $Version; executable = 'bri-client.exe'; content_config = 'content/packages.json'; files = $entries }
    $manifestJson = ConvertTo-Json -InputObject $manifest -Depth 10
    [IO.File]::WriteAllText((Join-Path $releasePath 'MANIFEST.json'), $manifestJson + "`n", [Text.UTF8Encoding]::new($false))
    Write-Host "Created $releasePath"
    Write-Host "Copied $contentFiles native content files ($contentBytes bytes), release executable $($exeInfo.Length) bytes."
    Write-Host 'The manifest lists every package file except itself; verify with -VerifyPackage.'
} catch {
    $safeRoot = [IO.Path]::GetFullPath($DestinationRoot).TrimEnd([IO.Path]::DirectorySeparatorChar,[IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $safeTarget = [IO.Path]::GetFullPath($releasePath)
    if ($safeTarget.StartsWith($safeRoot,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $safeTarget -PathType Container)) {
        $targetInfo = Get-Item -LiteralPath $safeTarget
        if (($targetInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) { Remove-Item -LiteralPath $safeTarget -Recurse -Force -ErrorAction SilentlyContinue }
    }
    throw
}
