[CmdletBinding()]
param(
    [string]$RepoRoot = (Split-Path -Parent $PSScriptRoot),
    [string]$ExecutablePath,
    [string]$DestinationRoot,
    [string]$Version,
    [string]$ExpectedExecutableSha256,
    [switch]$ValidateOnly,
    [string]$VerifyPackage
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

function Get-SourceDefaults([string]$Root) {
    $sourcePath = Join-Path $Root 'crates/client/src/content.rs'
    if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) { throw "Missing ContentConfig source: $sourcePath" }
    $source = [IO.File]::ReadAllText($sourcePath)
    $values = [ordered]@{ schema_version = 1 }
    foreach ($field in $script:PackFields) {
        $pattern = '(?m)^\s*' + [regex]::Escape($field) + ':\s*"([^"]+)"\.into\(\),'
        $match = [regex]::Match($source, $pattern)
        if (-not $match.Success) { throw "Cannot safely parse ContentConfig::default for '$field'; update packager with the runtime change." }
        $values[$field] = $match.Groups[1].Value
    }
    return $values
}

function Get-EffectiveContentConfig([string]$Root) {
    $defaults = Get-SourceDefaults $Root
    $contentRoot = Join-Path $Root 'content'
    $configPath = Join-Path $contentRoot 'client-content.json'
    if (Test-Path -LiteralPath $configPath -PathType Leaf) {
        $info = Get-Item -LiteralPath $configPath
        if (($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'content/client-content.json must not be a symbolic link.' }
        if ($info.Length -gt 1048576) { throw 'content/client-content.json exceeds 1 MiB.' }
        $override = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
        $allowed = @('schema_version') + $script:PackFields
        foreach ($property in $override.PSObject.Properties) {
            if ($property.Name -notin $allowed) { throw "Unknown ContentConfig field '$($property.Name)' would make the client reject the package." }
        }
        $schemaProperty = $override.PSObject.Properties['schema_version']
        if ($null -ne $schemaProperty -and [int]$schemaProperty.Value -ne 1) { throw 'Unsupported ContentConfig schema version.' }
        foreach ($field in $script:PackFields) {
            $property = $override.PSObject.Properties[$field]
            if ($null -ne $property) { $defaults[$field] = [string]$property.Value }
        }
    }
    foreach ($field in $script:PackFields) {
        $name = [string]$defaults[$field]
        if ([string]::IsNullOrWhiteSpace($name) -or $name.Contains('\') -or $name.Contains(':') -or
            @($name.Split('/') | Where-Object { $_ -in @('','.', '..') -or $_.EndsWith(' ') -or $_.EndsWith('.') }).Count -gt 0) {
            throw "Unsafe/empty ContentConfig package path for '$field': $name"
        }
        $defaults[$field] = $name
    }
    return $defaults
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
$config = Get-EffectiveContentConfig $RepoRoot
$sourceContent = [IO.Path]::GetFullPath((Join-Path $RepoRoot 'content'))
$selected = @()
$contentBytes = 0L
$contentFiles = 0
foreach ($field in $script:PackFields) {
    $name = [string]$config[$field]
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

$docInputs = @(
    @{ source = (Join-Path $RepoRoot 'docs/PLAYTEST.md'); destination = 'PLAYTEST.md' },
    @{ source = (Join-Path $RepoRoot 'docs/KNOWN-ISSUES.md'); destination = 'KNOWN-ISSUES.md' },
    @{ source = (Join-Path $PSScriptRoot 'Launch-Playtest.ps1'); destination = 'Launch-Playtest.ps1' },
    @{ source = (Join-Path $PSScriptRoot 'Launch-Playtest.cmd'); destination = 'Launch.cmd' }
)
foreach ($input in $docInputs) { if (-not (Test-Path -LiteralPath $input.source -PathType Leaf)) { throw "Required package file is missing: $($input.source)" } }
if ($ValidateOnly) {
    $configSource = 'ContentConfig::default parsed from source'
    if (Test-Path -LiteralPath (Join-Path $sourceContent 'client-content.json')) { $configSource = 'content/client-content.json override' }
    [pscustomobject]@{ selected_packages = $selected; content_files = $contentFiles; content_bytes = $contentBytes;
        executable_bytes = $exeInfo.Length; estimated_package_bytes = [long]$contentBytes + [long]$exeInfo.Length;
        executable_sha256 = $executableSha256; package_count = $selected.Count; content_config_source = $configSource } | ConvertTo-Json -Depth 6
    return
}
if ([string]::IsNullOrWhiteSpace($Version) -or $Version -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') { throw 'Supply -Version using 1–64 letters, digits, dot, underscore or dash.' }
if ([string]::IsNullOrWhiteSpace($ExpectedExecutableSha256) -or $ExpectedExecutableSha256 -notmatch '^[0-9A-Fa-f]{64}$') { throw 'Supply the SHA-256 reported for the root-provided release executable using -ExpectedExecutableSha256.' }
if ($executableSha256 -cne $ExpectedExecutableSha256.ToLowerInvariant()) { throw "Release executable hash differs from root's expected build: $executableSha256" }
[IO.Directory]::CreateDirectory($DestinationRoot) | Out-Null
$releasePath = Join-Path $DestinationRoot "BlocklandReImagined-alpha-$Version"
if (Test-Path -LiteralPath $releasePath) { throw "Refusing to overwrite an existing playtest release: $releasePath" }
[IO.Directory]::CreateDirectory($releasePath) | Out-Null
try {
    Copy-Item -LiteralPath $ExecutablePath -Destination (Join-Path $releasePath 'bri-client.exe')
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
    $effective = [ordered]@{ schema_version = 1 }
    foreach ($field in $script:PackFields) { $effective[$field] = $config[$field] }
    $configJson = ConvertTo-Json -InputObject $effective -Depth 5
    [IO.File]::WriteAllText((Join-Path $packagedContent 'client-content.json'), $configJson + "`n", [Text.UTF8Encoding]::new($false))
    $entries = Get-ManifestEntries $releasePath
    $manifest = [ordered]@{ schema_version = 1; version = $Version; executable = 'bri-client.exe'; content_config = 'content/client-content.json'; files = $entries }
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
