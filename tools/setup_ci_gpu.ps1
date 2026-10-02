# Provision pinned Microsoft software graphics tools for Windows CI only.
# The same WARP/DXC sources are used by gfx-rs/wgpu's upstream GPU tests.
param([string]$TargetDir = 'target/debug')
$ErrorActionPreference = 'Stop'
$stage = Join-Path $env:RUNNER_TEMP 'bri-ci-gpu'
New-Item -ItemType Directory -Force $stage | Out-Null
$packages = @(
    @{ name = 'warp'; url = 'https://www.nuget.org/api/v2/package/Microsoft.Direct3D.WARP/1.0.20'; sha256 = 'e5fe5de661ce98b58ef9cfb736e73c0a7a2623d3bbf5f14839b2d55566d87e40'; files = @('build/native/bin/x64/d3d10warp.dll') },
    @{ name = 'dxc'; url = 'https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.9.2602.24/dxc_2026_05_27.zip'; sha256 = 'cf658aacf070d3045e31b8f1f8a696c2945f37c1095019481ef7c513368db3b4'; files = @('bin/x64/dxcompiler.dll', 'bin/x64/dxil.dll') }
)
foreach ($package in $packages) {
    $archive = Join-Path $stage ($package.name + '.zip')
    Invoke-WebRequest $package.url -OutFile $archive -TimeoutSec 120
    $digest = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($digest -cne $package.sha256) { throw "CI graphics archive checksum mismatch: $($package.name)" }
    $unpacked = Join-Path $stage $package.name
    Expand-Archive $archive $unpacked -Force
    foreach ($destination in @($TargetDir, (Join-Path $TargetDir 'deps'))) {
        New-Item -ItemType Directory -Force $destination | Out-Null
        foreach ($file in $package.files) {
            Copy-Item -LiteralPath (Join-Path $unpacked $file) -Destination $destination
        }
    }
}
Write-Output 'Provisioned Microsoft WARP 1.0.20 and DXC v1.9.2602.24 for CI tests.'
