$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-env.ps1')
Push-Location $repoRoot
try {
    npm run tauri dev
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
