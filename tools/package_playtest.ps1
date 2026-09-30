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
    # content/packages.json.
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
    # The standalone launcher (crates/launcher), built with the client. The
    # release also ships as <release>.zip and as one self-contained
    # <release>-standalone/BlocklandReImagined.exe carrying that zip.
    [string]$LauncherName = 'BlocklandReImagined.exe',
    [switch]$NoStandalone,
    # Check a standalone exe: its payload's hash and the release inside it.
    [string]$VerifyStandalone,
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

# The default Add-Ons every release ships, in load order
# (packages/default-addons.json: the Duplicator, the Stunt Plane and the
# Mirror turned on; the Ragdoll and the Gravity Gun turned off). Each is
# committed under packages/<path>; releases carry it as content/addons/<id>.
# The game installs the same ones into a source checkout's content
# (crates/package/src/defaults.rs).
function Get-DefaultAddOns([string]$Repo) {
    $path = Join-Path $Repo 'packages/default-addons.json'
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing the default Add-On list: $path" }
    $list = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    if ([int]$list.schema_version -ne 1) { throw "Unsupported schema in $path." }
    return @($list.addons)
}

# Whether a default Add-On ships turned on (it does unless the list says
# "enabled": false, like the Ragdoll).
function Test-DefaultAddOnOn($AddOn) {
    $enabled = $AddOn.PSObject.Properties['enabled']
    return ($null -eq $enabled -or [bool]$enabled.Value)
}

# Why $Directory is not a whole copy of the default Add-On (empty when it
# is): its manifest names it and, for an imported one, it came from the
# listed archive, at the listed version, with its vehicles.
function Get-DefaultAddOnProblems([string]$Directory, $AddOn) {
    $manifestPath = Join-Path $Directory 'package.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { return @("$Directory has no package.json") }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $problems = @()
    if ([string]$manifest.id -cne [string]$AddOn.id) { $problems += "$manifestPath names '$($manifest.id)', not '$($AddOn.id)'" }
    $import = $AddOn.PSObject.Properties['import']
    if ($null -ne $import) {
        $import = $import.Value
        $provenance = $manifest.PSObject.Properties['provenance']
        if ($null -eq $provenance -or -not (ConvertTo-Json $provenance.Value -Compress).Contains([string]$import.archive_sha256)) {
            $problems += "$manifestPath was not imported from the listed $($import.archive)"
        }
        if ([string]$manifest.version -cne [string]$import.version) { $problems += "$manifestPath is version $($manifest.version), not $($import.version)" }
        $vehiclesPath = Join-Path $Directory 'assets/vehicles.json'
        $ids = @()
        if (Test-Path -LiteralPath $vehiclesPath -PathType Leaf) { $ids = @((Get-Content -LiteralPath $vehiclesPath -Raw | ConvertFrom-Json).definitions | ForEach-Object { [string]$_.id }) }
        foreach ($vehicle in @($import.vehicles)) { if ($ids -notcontains [string]$vehicle) { $problems += "$Directory lacks vehicle $vehicle" } }
    }
    return $problems
}

# A release turns on every default Add-On: listed in content/packages.json
# at addons/<id>, and whole there.
function Verify-DefaultAddOns([string]$Root) {
    $listPath = Join-Path $Root 'content/packages.json'
    if (-not (Test-Path -LiteralPath $listPath -PathType Leaf)) { throw 'The release has no content/packages.json.' }
    $enabled = @((Read-PackageList $listPath).packages)
    $defaults = @(Get-DefaultAddOns $RepoRoot)
    foreach ($addOn in $defaults) {
        $entry = @($enabled | Where-Object { [string]$_.id -ceq [string]$addOn.id })
        if (-not (Test-DefaultAddOnOn $addOn)) {
            # Carried turned off: installed, not listed.
            if ($entry.Count -ne 0) { throw "The release turns on $($addOn.id), which ships turned off." }
        } elseif ($entry.Count -ne 1 -or [string]$entry[0].dir -cne "addons/$($addOn.id)") { throw "The release does not turn on the default Add-On $($addOn.id) at addons/$($addOn.id)." }
        $problems = @(Get-DefaultAddOnProblems (Join-Path $Root "content/addons/$($addOn.id)") $addOn)
        if ($problems.Count -gt 0) { throw "Default Add-On $($addOn.id) is incomplete: $($problems -join '; ')" }
    }
    Write-Host "Verified default Add-Ons: $(($defaults | ForEach-Object { $_.id }) -join ', ')."
}

# The list entry and files of the Add-On in $Directory, carried to
# content/$Prefix/<id>. Keep the side in step with bri_package::library's
# side_for_kinds: server kinds only, client kinds (model, hud) only, else
# shared. An Add-On that is only client code (no provides) is presentation:
# client.
function New-ModPackage([string]$Directory, [string]$Prefix) {
    $manifest = Get-Content -LiteralPath (Join-Path $Directory 'package.json') -Raw | ConvertFrom-Json
    $files = @(Get-PackageFiles $Directory)
    # Strict mode: a client-code-only Add-On has no provides at all.
    $provides = $manifest.PSObject.Properties['provides']
    $kinds = @($(if ($provides) { $provides.Value }) | Where-Object { $_ } | ForEach-Object { [string]$_.kind })
    $side = if ($kinds.Count -eq 0 -and $null -ne $manifest.PSObject.Properties['client']) { 'client' }
        elseif ($kinds.Count -eq 0) { 'shared' }
        elseif (@($kinds | Where-Object { $_ -notin @('behaviour','script','world','entity','mode','archetype') }).Count -eq 0) { 'server' }
        elseif (@($kinds | Where-Object { $_ -notin @('model','hud') }).Count -eq 0) { 'client' }
        else { 'shared' }
    return [pscustomobject]@{ id = [string]$manifest.id; version = [string]$manifest.version; side = $side; path = $Directory; dir = "$Prefix/$($manifest.id)"; files = $files.Count }
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
    Verify-DefaultAddOns $root
}

# Standalone exe footer (crates/launcher): payload SHA-256, u64 length, magic.
$script:StandaloneMagic = [Text.Encoding]::ASCII.GetBytes('BRISFX01')

# Zip the release folder under its own name. Entries use forward slashes
# (ZipFile.CreateFromDirectory on Windows PowerShell writes backslashes).
function New-ReleaseZip([string]$Folder,[string]$Zip) {
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $top = Split-Path -Leaf $Folder
    $paths = @(Get-PackageFiles $Folder | ForEach-Object { Get-RelativePackagePath $Folder $_.FullName })
    [Array]::Sort($paths, [StringComparer]::Ordinal)
    $archive = [IO.Compression.ZipFile]::Open($Zip, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $archive.CreateEntry("$top/") | Out-Null
        foreach ($relative in $paths) {
            $file = Join-Path $Folder ($relative.Replace('/',[IO.Path]::DirectorySeparatorChar))
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file, "$top/$relative", [IO.Compression.CompressionLevel]::Optimal) | Out-Null
        }
    } finally { $archive.Dispose() }
}

function New-StandaloneExe([string]$Stub,[string]$Zip,[string]$Out) {
    $sha = [Security.Cryptography.SHA256]::Create()
    $output = [IO.File]::Open($Out,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
    try {
        $stubStream = [IO.File]::OpenRead($Stub)
        try { $stubStream.CopyTo($output) } finally { $stubStream.Dispose() }
        $zipStream = [IO.File]::OpenRead($Zip)
        try { $hash = $sha.ComputeHash($zipStream); $zipStream.Position = 0; $zipStream.CopyTo($output); $length = $zipStream.Length } finally { $zipStream.Dispose() }
        $output.Write($hash,0,32)
        $output.Write([BitConverter]::GetBytes([UInt64]$length),0,8)
        $output.Write($script:StandaloneMagic,0,8)
    } finally { $output.Dispose(); $sha.Dispose() }
}

# Where the payload ends: before an Authenticode signature, if any.
function Get-StandaloneEnd([IO.BinaryReader]$Reader) {
    $stream = $Reader.BaseStream
    $end = $stream.Length
    if ($end -lt 64) { return $end }
    $stream.Position = 0x3c; $pe = $Reader.ReadUInt32()
    if ($pe + 26 -gt $end) { return $end }
    $stream.Position = $pe
    if ($Reader.ReadUInt32() -ne 0x4550) { return $end }
    $stream.Position = $pe + 24
    $magic = $Reader.ReadUInt16()
    $directories = if ($magic -eq 0x20b) { $pe + 24 + 112 } elseif ($magic -eq 0x10b) { $pe + 24 + 96 } else { return $end }
    $stream.Position = $directories + 32
    $offset = $Reader.ReadUInt32(); $size = $Reader.ReadUInt32()
    if ($offset -gt 0 -and $size -gt 0 -and $offset + $size -le $end) { return [long]$offset }
    return $end
}

function Verify-StandaloneExe([string]$Path) {
    $exe = (Resolve-Path -LiteralPath $Path).Path
    $stream = [IO.File]::OpenRead($exe)
    $reader = [IO.BinaryReader]::new($stream)
    $temp = Join-Path ([IO.Path]::GetTempPath()) "bri-standalone-verify-$([Guid]::NewGuid().ToString('N'))"
    try {
        $end = Get-StandaloneEnd $reader
        $found = $false
        foreach ($pad in 0..7) {
            $at = $end - 48 - $pad
            if ($at -lt 0) { break }
            $stream.Position = $at
            $footer = $reader.ReadBytes(48)
            if ([Text.Encoding]::ASCII.GetString($footer,40,8) -eq 'BRISFX01') { $found = $true; break }
        }
        if (-not $found) { throw "No game payload in $exe." }
        $length = [BitConverter]::ToUInt64($footer,32)
        if ($length -gt $at) { throw 'Standalone payload is truncated.' }
        [IO.Directory]::CreateDirectory($temp) | Out-Null
        $zip = Join-Path $temp 'payload.zip'
        $out = [IO.File]::Create($zip)
        try {
            $stream.Position = $at - $length
            $buffer = New-Object byte[] 1048576
            $left = [long]$length
            while ($left -gt 0) {
                $n = $stream.Read($buffer,0,[int][Math]::Min($buffer.Length,$left))
                if ($n -le 0) { throw 'Standalone payload is truncated.' }
                $out.Write($buffer,0,$n); $left -= $n
            }
        } finally { $out.Dispose() }
        $expected = -join ($footer[0..31] | ForEach-Object { $_.ToString('x2') })
        $actual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -cne $expected) { throw "Standalone payload checksum mismatch: $actual" }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $unpacked = Join-Path $temp 'release'
        [IO.Compression.ZipFile]::ExtractToDirectory($zip,$unpacked)
        $tops = @(Get-ChildItem -LiteralPath $unpacked -Force)
        if ($tops.Count -ne 1 -or -not $tops[0].PSIsContainer) { throw 'Standalone payload must hold exactly one release folder.' }
        Verify-PlaytestPackage $tops[0].FullName
        Write-Host "Verified standalone payload $expected ($length bytes)."
    } finally {
        $reader.Dispose()
        if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    }
}

if (-not [string]::IsNullOrWhiteSpace($VerifyPackage)) {
    Verify-PlaytestPackage $VerifyPackage
    return
}
if (-not [string]::IsNullOrWhiteSpace($VerifyStandalone)) {
    Verify-StandaloneExe $VerifyStandalone
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
$launcherPath = $null
if (-not $NoStandalone) {
    if ($LauncherName -notmatch '^[A-Za-z0-9._-]+\.exe$') { throw "Launcher must be a bare .exe name: $LauncherName" }
    $launcherPath = Join-Path (Split-Path -Parent $ExecutablePath) $LauncherName
    if (-not (Test-Path -LiteralPath $launcherPath -PathType Leaf)) { throw "Standalone launcher is missing; build it with the client (cargo build --release -p bri-launcher) or pass -NoStandalone: $launcherPath" }
}
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

# The default Add-Ons every build ships (content/addons/<id>), from
# packages/default-addons.json, turned on unless the list carries one turned
# off (the Ragdoll and the Gravity Gun). The Stress Lab ones join them with
# -StressLab. The Steel Ball (packages/showcase) stays out of releases; the
# bri-package test every_showcase_add_on_ships_turned_off_or_is_held_back
# keeps the showcase Add-Ons and this list in step.
$modPackages = @()
foreach ($addOn in Get-DefaultAddOns $RepoRoot) {
    $directory = Join-Path (Join-Path $RepoRoot 'packages') ([string]$addOn.path)
    $problems = @(Get-DefaultAddOnProblems $directory $addOn)
    if ($problems.Count -gt 0) { throw "Default Add-On $($addOn.id) is missing or incomplete: $($problems -join '; ')" }
    $mod = New-ModPackage $directory 'addons'
    $mod | Add-Member -NotePropertyName enabled -NotePropertyValue (Test-DefaultAddOnOn $addOn)
    $modPackages += $mod
}
if ($StressLab) {
    # Not $stressLab: PowerShell names are case-insensitive, and that one is the -StressLab switch.
    $stressLabRoot = Join-Path $RepoRoot 'packages/stresslab'
    $found = @(Get-ChildItem -LiteralPath $stressLabRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name | Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'package.json') -PathType Leaf })
    if ($found.Count -eq 0) { throw "No Add-Ons found in $stressLabRoot" }
    foreach ($dir in $found) { $modPackages += New-ModPackage $dir.FullName 'stresslab' }
}

$docInputs = @(
    @{ source = (Join-Path $RepoRoot 'docs/PLAYTEST.md'); destination = 'PLAYTEST.md' },
    @{ source = (Join-Path $RepoRoot 'docs/KNOWN-ISSUES.md'); destination = 'KNOWN-ISSUES.md' },
    @{ source = (Join-Path $RepoRoot 'docs/TESTER-GUIDE.md'); destination = 'TESTER-GUIDE.md' },
    @{ source = (Join-Path $RepoRoot 'docs/FEATURES.md'); destination = 'FEATURES.md' },
    @{ source = (Join-Path $PSScriptRoot 'Launch-Playtest.ps1'); destination = 'Launch-Playtest.ps1' },
    @{ source = (Join-Path $PSScriptRoot 'Launch-Playtest.cmd'); destination = 'Launch.cmd' }
)
if ($StressLab) { $docInputs += @{ source = (Join-Path $RepoRoot 'docs/stress-lab/PLAYTEST-STRESS-LAB.md'); destination = 'PLAYTEST-STRESS-LAB.md' } }
foreach ($input in $docInputs) { if (-not (Test-Path -LiteralPath $input.source -PathType Leaf)) { throw "Required package file is missing: $($input.source)" } }
if ($ValidateOnly) {
    $configSource = $effective.source
    [pscustomobject]@{ selected_packages = $selected; content_files = $contentFiles; content_bytes = $contentBytes;
        executable_bytes = $exeInfo.Length; estimated_package_bytes = [long]$contentBytes + [long]$exeInfo.Length;
        executable_sha256 = $executableSha256; package_count = $selected.Count; content_config_source = $configSource; mod_packages = $modPackages; companion_executables = $companions;
        standalone_launcher = $launcherPath } | ConvertTo-Json -Depth 6
    return
}
if ([string]::IsNullOrWhiteSpace($Version) -or $Version -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') { throw 'Supply -Version using 1–64 letters, digits, dot, underscore or dash.' }
if ([string]::IsNullOrWhiteSpace($ExpectedExecutableSha256) -or $ExpectedExecutableSha256 -notmatch '^[0-9A-Fa-f]{64}$') { throw 'Supply the SHA-256 reported for the root-provided release executable using -ExpectedExecutableSha256.' }
if ($executableSha256 -cne $ExpectedExecutableSha256.ToLowerInvariant()) { throw "Release executable hash differs from root's expected build: $executableSha256" }
$buildVersion = if ($SkipVersionCheck) { $Version } else { Get-BuildVersion $ExecutablePath }
if ($buildVersion -cne $Version) { throw "The executable reports version '$buildVersion', not '$Version'. Rebuild with `$env:BRI_VERSION = '$Version' before cargo build --release." }
if (-not [string]::IsNullOrWhiteSpace($SignCertificateThumbprint) -and $SignCertificateThumbprint -notmatch '^[0-9A-Fa-f]{40}$') { throw 'Supply -SignCertificateThumbprint as the 40-hex-digit SHA-1 thumbprint.' }
[IO.Directory]::CreateDirectory($DestinationRoot) | Out-Null
$releasePath = Join-Path $DestinationRoot "BlocklandReImagined-$Version-windows"
$zipPath = "$releasePath.zip"
$standaloneDir = "$releasePath-standalone"
$standaloneExe = Join-Path $standaloneDir 'BlocklandReImagined.exe'
foreach ($existing in @($releasePath, $zipPath, $standaloneDir)) {
    if (Test-Path -LiteralPath $existing) { throw "Refusing to overwrite an existing playtest release: $existing" }
}
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
        $destination = Join-Path $packagedContent $mod.dir
        [IO.Directory]::CreateDirectory($destination) | Out-Null
        foreach ($child in Get-ChildItem -LiteralPath $mod.path -Force) {
            Copy-Item -LiteralPath $child.FullName -Destination (Join-Path $destination $child.Name) -Recurse
        }
        if ($null -ne $mod.PSObject.Properties['enabled'] -and -not $mod.enabled) { continue }
        $list.packages += [pscustomobject][ordered]@{ id = $mod.id; version = $mod.version; side = $mod.side; dir = $mod.dir }
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
    # The folder as one zip (what testers download), and the same zip inside
    # one exe that unpacks itself per user (docs/playtest-package-layout.md).
    New-ReleaseZip $releasePath $zipPath
    Write-Host "Created $zipPath ($((Get-Item -LiteralPath $zipPath).Length) bytes)"
    if (-not $NoStandalone) {
        [IO.Directory]::CreateDirectory($standaloneDir) | Out-Null
        New-StandaloneExe $launcherPath $zipPath $standaloneExe
        if (-not [string]::IsNullOrWhiteSpace($SignCertificateThumbprint)) { Invoke-CodeSigning $standaloneDir }
        Verify-StandaloneExe $standaloneExe
        Write-Host "Created $standaloneExe ($((Get-Item -LiteralPath $standaloneExe).Length) bytes)"
    }
} catch {
    foreach ($partial in @($zipPath, $standaloneExe)) { if (Test-Path -LiteralPath $partial -PathType Leaf) { Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue } }
    if (Test-Path -LiteralPath $standaloneDir -PathType Container) { Remove-Item -LiteralPath $standaloneDir -Force -ErrorAction SilentlyContinue }
    $safeRoot = [IO.Path]::GetFullPath($DestinationRoot).TrimEnd([IO.Path]::DirectorySeparatorChar,[IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $safeTarget = [IO.Path]::GetFullPath($releasePath)
    if ($safeTarget.StartsWith($safeRoot,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $safeTarget -PathType Container)) {
        $targetInfo = Get-Item -LiteralPath $safeTarget
        if (($targetInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) { Remove-Item -LiteralPath $safeTarget -Recurse -Force -ErrorAction SilentlyContinue }
    }
    throw
}
