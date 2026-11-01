# Builds Painty, writes the checksum used by the updater, and creates the installer.
# Output (in dist\): painty.exe, painty.exe.sha256, PaintySetup-<version>.exe
# Upload painty.exe and painty.exe.sha256 to the GitHub release: the updater needs both.
$ErrorActionPreference = 'Stop'
Set-Location -Path $PSScriptRoot
$Host.UI.RawUI.WindowTitle = "Painty Build"

# Version comes from Cargo.toml, so it is written in one place only.
$match = Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $match) { throw "Version not found in Cargo.toml" }
$version = $match.Matches.Groups[1].Value

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit code $LASTEXITCODE)" }

New-Item dist -ItemType Directory -Force | Out-Null
Copy-Item target\release\painty.exe dist\painty.exe -Force
$hash = (Get-FileHash dist\painty.exe -Algorithm SHA256).Hash.ToLower()
"$hash  painty.exe" | Set-Content -Path dist\painty.exe.sha256 -Encoding ascii

# Inno Setup: PATH first, then the usual install folders.
$candidates = @(
    (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source,
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
)
$iscc = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup (ISCC.exe) not found" }

& $iscc "/DMyAppVersion=$version" "painty.iss"
if ($LASTEXITCODE -ne 0) { throw "ISCC failed (exit code $LASTEXITCODE)" }
