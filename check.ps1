# Runs the project verification commands used by CI.
$ErrorActionPreference = 'Stop'
Set-Location -Path $PSScriptRoot
$Host.UI.RawUI.WindowTitle = "Painty Check"

cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed (exit code $LASTEXITCODE)" }

cargo clippy --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed (exit code $LASTEXITCODE)" }

cargo test
if ($LASTEXITCODE -ne 0) { throw "cargo test failed (exit code $LASTEXITCODE)" }
