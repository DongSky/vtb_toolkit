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

$bundleDir = Join-Path $projectRoot "target\release\bundle"
if (Test-Path $bundleDir) {
    Remove-Item -Recurse -Force $bundleDir
}

npm ci
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

npm run tauri build -- --bundles nsis,msi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Windows bundles: $projectRoot\target\release\bundle"
