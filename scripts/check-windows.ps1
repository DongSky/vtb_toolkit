$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "windows-env.ps1")

Push-Location $repoRoot
try {
    cargo check -p vtb-toolkit
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
