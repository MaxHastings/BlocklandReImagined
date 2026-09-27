[CmdletBinding()]
param(
    [string]$Address,
    [string]$CertificatePath = (Join-Path $PSScriptRoot 'user-state/host-certificate.der'),
    [string]$StateDirectory = (Join-Path $PSScriptRoot 'user-state'),
    [switch]$ReplaceExisting,
    [switch]$NonInteractive
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-IpEndpoint([string]$Value) {
    if ($Value -match '^\[(?<ip>[0-9A-Fa-f:.%]+)\]:(?<port>[0-9]{1,5})$') {
        $ipText = $Matches.ip
    } elseif ($Value -match '^(?<ip>(?:[0-9]{1,3}\.){3}[0-9]{1,3}):(?<port>[0-9]{1,5})$') {
        $ipText = $Matches.ip
    } else {
        throw 'Enter an IP literal and port, such as 192.168.1.10:28000 (hostnames are not accepted).'
    }
    $ip = $null
    if (-not [Net.IPAddress]::TryParse($ipText, [ref]$ip)) { throw 'Invalid IP address.' }
    $port = 0
    if (-not [int]::TryParse($Matches.port, [ref]$port) -or $port -lt 1 -or $port -gt 65535) { throw 'Port must be in the range 1–65535.' }
    if ($ip.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetworkV6) {
        return "[$($ip.ToString())]:$port"
    }
    if ($ip.AddressFamily -ne [Net.Sockets.AddressFamily]::InterNetwork) { throw 'Unsupported IP address family.' }
    return "$($ip.ToString()):$port"
}

function Read-BoundedBytes([string]$Path, [long]$Limit) {
    $item = Get-Item -LiteralPath $Path -ErrorAction Stop
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Expected a regular file (not a link): $Path"
    }
    if ($item.Length -lt 1 -or $item.Length -gt $Limit) { throw "File size is outside the allowed range: $Path" }
    return [IO.File]::ReadAllBytes($item.FullName)
}
function Test-ByteArrayEqual([byte[]]$Left, [byte[]]$Right) {
    if ($Left.Length -ne $Right.Length) { return $false }
    for ($i = 0; $i -lt $Left.Length; $i++) { if ($Left[$i] -ne $Right[$i]) { return $false } }
    return $true
}
function Format-Sha256([byte[]]$Bytes) {
    return [BitConverter]::ToString($Bytes).Replace('-', '')
}

if ([string]::IsNullOrWhiteSpace($Address)) {
    if ($NonInteractive) { throw '-Address is required with -NonInteractive.' }
    $Address = Read-Host 'Host IP address and port'
}
$endpoint = Resolve-IpEndpoint $Address
$certificatePath = [IO.Path]::GetFullPath($CertificatePath)
$stateDirectory = [IO.Path]::GetFullPath($StateDirectory)
$certificateBytes = [byte[]](Read-BoundedBytes $certificatePath 16384)
$certificate = $null
try {
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($certificateBytes)
    if (-not (Test-ByteArrayEqual ([byte[]]$certificate.RawData) ([byte[]]$certificateBytes))) {
        throw 'Input is not a single canonical DER X.509 certificate.'
    }
    $now = [DateTime]::UtcNow
    if ($certificate.NotBefore.ToUniversalTime() -gt $now -or $certificate.NotAfter.ToUniversalTime() -le $now) {
        throw 'The certificate is not currently valid.'
    }
} catch {
    throw "Cannot validate host certificate DER: $($_.Exception.Message)"
}
$sha = [Security.Cryptography.SHA256]::Create()
try { $fingerprint = Format-Sha256 ($sha.ComputeHash($certificateBytes)) }
finally { $sha.Dispose() }
Write-Host "Host: $endpoint"
Write-Host "Certificate SHA-256: $($fingerprint -replace '(.{2})(?!$)', '$1:')"
Write-Host 'This pins this exact certificate for this IP and port; it does not discover or verify the host through a certificate authority.'

[IO.Directory]::CreateDirectory($stateDirectory) | Out-Null
$pinPath = Join-Path $stateDirectory 'trusted-hosts.json'
$pins = [ordered]@{}
if (Test-Path -LiteralPath $pinPath) {
    $pinFile = Get-Item -LiteralPath $pinPath
    if (($pinFile.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'trusted-hosts.json must not be a symbolic link.' }
    $pinBytes = Read-BoundedBytes $pinPath 1048576
    $text = [Text.Encoding]::UTF8.GetString($pinBytes)
    $existing = ConvertFrom-Json -InputObject $text
    if ($null -eq $existing -or $existing -isnot [pscustomobject]) { throw 'Host pin file must be a JSON object.' }
    $properties = @($existing.PSObject.Properties)
    if ($properties.Count -gt 128) { throw 'Host pin count exceeds 128 entries.' }
    foreach ($property in $properties) {
        $key = Resolve-IpEndpoint ([string]$property.Name)
        $value = @($property.Value)
        if ($value.Count -lt 1 -or $value.Count -gt 16384) { throw "Invalid existing certificate pin for $key." }
        $bytes = [Collections.Generic.List[byte]]::new()
        foreach ($part in $value) {
            $number = 0
            if (-not [int]::TryParse([string]$part, [ref]$number) -or $number -lt 0 -or $number -gt 255) {
                throw "Invalid DER byte in existing pin for $key."
            }
            $bytes.Add([byte]$number)
        }
        $pins[$key] = $bytes.ToArray()
    }
}

if ($pins.Contains($endpoint)) {
    $old = [byte[]]$pins[$endpoint]
    $oldSha = [Security.Cryptography.SHA256]::Create()
    try { $oldFingerprint = Format-Sha256 ($oldSha.ComputeHash($old)) }
    finally { $oldSha.Dispose() }
    if (Test-ByteArrayEqual ([byte[]]$old) ([byte[]]$certificateBytes)) {
        Write-Host 'This exact certificate is already trusted; no file change was made.'
        return
    }
    Write-Host "Existing certificate SHA-256: $($oldFingerprint -replace '(.{2})(?!$)', '$1:')"
    if (-not $ReplaceExisting) {
        if ($NonInteractive) { throw 'The host certificate changed. Review the fingerprint, then rerun with -ReplaceExisting to replace the pin.' }
        $answer = Read-Host 'Replace the existing pin for this endpoint? Type REPLACE to continue'
        if ($answer -cne 'REPLACE') { throw 'Pin replacement was not confirmed.' }
    }
}
$pins[$endpoint] = [byte[]]$certificateBytes
if ($pins.Count -gt 128) { throw 'Host pin count exceeds 128 entries.' }
$json = ConvertTo-Json -InputObject $pins -Depth 5
$output = [Text.Encoding]::UTF8.GetBytes($json + "`n")
if ($output.Length -gt 1048576) { throw 'Updated host pin file exceeds 1 MiB.' }
$temporary = Join-Path $stateDirectory ".trusted-hosts-$PID-$([Guid]::NewGuid().ToString('N')).tmp"
$stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $stream.Write($output, 0, $output.Length); $stream.Flush($true) }
finally { $stream.Dispose() }
try {
    if (Test-Path -LiteralPath $pinPath) {
        $backup = "$pinPath.$PID.bak"
        [IO.File]::Replace($temporary, $pinPath, $backup)
        Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
    }
    else { [IO.File]::Move($temporary, $pinPath) }
} catch {
    Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
    throw
}
Write-Host "Trusted host pin saved atomically to $pinPath"
