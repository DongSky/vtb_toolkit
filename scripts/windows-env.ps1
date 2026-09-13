# Shared Windows native build environment; dot-source from build/check/dev.
$repoRoot = Split-Path -Parent $PSScriptRoot
$compatibleLlvm = Join-Path $repoRoot '.tools\libclang18\clang\native'
if (Test-Path -LiteralPath (Join-Path $compatibleLlvm 'libclang.dll')) {
    $env:LIBCLANG_PATH = $compatibleLlvm
} elseif (-not $env:LIBCLANG_PATH) {
    foreach ($candidate in @((Join-Path $repoRoot '.tools\llvm\bin'), 'C:\Program Files\LLVM\bin')) {
        if (Test-Path -LiteralPath (Join-Path $candidate 'libclang.dll')) {
            $env:LIBCLANG_PATH = $candidate
            break
        }
    }
}
if (-not $env:LIBCLANG_PATH) {
    throw 'libclang.dll not found. Install LLVM 18 or set LIBCLANG_PATH to a compatible libclang directory.'
}
if (-not $env:CMAKE_GENERATOR) { $env:CMAKE_GENERATOR = 'Visual Studio 17 2022' }
$env:BINDGEN_EXTRA_CLANG_ARGS = '--target=x86_64-pc-windows-msvc'
Remove-Item Env:WHISPER_DONT_GENERATE_BINDINGS -ErrorAction SilentlyContinue
