# Removes the Cargo build artifacts for Painty.
$ErrorActionPreference = 'Stop'
Set-Location -Path $PSScriptRoot
$Host.UI.RawUI.WindowTitle = "Painty Clean"

cargo clean
if ($LASTEXITCODE -ne 0) { throw "cargo clean failed (exit code $LASTEXITCODE)" }
