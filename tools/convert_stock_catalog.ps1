param(
    [Parameter(Mandatory=$true)][string]$V20Root,
    [string]$ConvertedContent = 'content/maps-pass-008',
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [string]$StockScript = '.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs',
    [string]$DefaultAddonList = '.research/bl-decompiled/v20/server/defaultAddOnList.cs'
)
$ErrorActionPreference = 'Stop'
# Read declarations as data. Never execute original scripts or write to the install.
$taskAddons = [System.Collections.Generic.List[string]]::new()
$taskNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($taskLine in Get-Content -LiteralPath $DefaultAddonList) {
    if ($taskLine -match '^\s*\$AddOn__(Brick_[A-Za-z0-9_]+)\s*=\s*1\s*;\s*$') {
        $taskName = $Matches[1]
        if (-not $taskNames.Add($taskName)) { throw "Repeated default brick add-on: $taskName" }
        $taskPath = Join-Path $V20Root "Add-Ons/$taskName.zip"
        if (-not (Test-Path -LiteralPath $taskPath -PathType Leaf)) { throw "Missing default brick add-on: $taskPath" }
        $taskAddons.Add($taskPath)
    }
}
if ($taskAddons.Count -eq 0) { throw 'No enabled stock brick declarations found' }
& cargo run -p bri-convert --locked --release --bin stock_catalog -- $StockScript $ConvertedContent $OutputDirectory @taskAddons
if ($LASTEXITCODE -ne 0) { throw "Stock catalog conversion failed: $LASTEXITCODE" }
