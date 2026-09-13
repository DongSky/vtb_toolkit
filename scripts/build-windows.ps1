$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot

foreach ($commandName in @("node", "npm", "cargo", "rustc")) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) {
        throw "Missing required command: $commandName"
    }
}

if (-not $IsWindows) {
    throw "This script must be run on Windows."
}

. (Join-Path $PSScriptRoot 'windows-env.ps1')

$bundleDir = Join-Path $projectRoot "target\release\bundle"
if (Test-Path $bundleDir) {
    $resolvedBundle = (Resolve-Path -LiteralPath $bundleDir).Path
    $expectedBundle = [System.IO.Path]::GetFullPath((Join-Path $projectRoot 'target\release\bundle'))
    if ($resolvedBundle -ne $expectedBundle) { throw 'Unexpected bundle directory; refusing cleanup.' }
    Remove-Item -LiteralPath $resolvedBundle -Recurse -Force
}

npm ci
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

npm run tauri build -- --bundles nsis,msi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Windows bundles: $projectRoot\target\release\bundle"
