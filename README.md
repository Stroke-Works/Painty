<p align="center">
  <img src="assets/icon.png" width="128" alt="Painty logo">
</p>

<h1 align="center">Painty</h1>

<p align="center">
  A minimal paint application for Windows, written in Rust with egui and eframe.
</p>

<p align="center">
  <a href="https://github.com/Stroke-Works/Painty/releases/latest"><b>Download</b></a>
  ·
  <a href="https://github.com/Stroke-Works/Painty/releases">Releases</a>
  ·
  <a href="ARCHITECTURE.md">Architecture</a>
  ·
  <a href="LICENSE">MIT License</a>
</p>

---

## Features

- **Brush** with an adjustable size and color picker
- **Smooth strokes** that follow the pointer without jitter
- **Eraser** that cuts through existing strokes
- **Line, rectangle and ellipse tools**, with optional shape filling
- **Eyedropper** for picking colors from the drawing
- **Editable projects** saved as `.painty` files, including the drawing, brush and palette
- **Canvas navigation** with zoom and pan
- **Customizable six-color palette**, saved between sessions
- **Undo and redo** (Ctrl+Z / Ctrl+Y or Ctrl+Shift+Z)
- **PNG and JPEG export** (Ctrl+Shift+S)
- **Clear all**, with confirmation
- **Light and dark themes**, with an independent light or dark canvas
- **Five languages**: English, French, Spanish, German and Italian
- **Responsive toolbar** that folds its controls in and out as the window is resized
- **Borderless window** with rounded corners and a compact floating panel: drag grip, minimize, maximize and close
- **Update channels**: stable by default, with an optional beta channel
- **Verified automatic updates** from GitHub Releases, checked against a SHA-256 before installation

## Install

1. Open the [latest release](https://github.com/Stroke-Works/Painty/releases/latest).
2. Download `PaintySetup-X.Y.Z.exe` and run it. Administrator rights are not required.
3. Windows can then offer Painty in **Open with** for `.painty` project files.

Painty checks for updates when it starts; this can be disabled in **Settings**. You can also check manually from **Settings → Check for updates**. Click the version in **Settings** to read the release notes bundled with the installed version.

## Updates

Painty uses two channels, selected in **Settings → Update channel**:

- **Stable** (default) follows the latest stable GitHub release.
- **Beta** also accepts pre-releases.

The updater downloads only `painty.exe`. It verifies the published `painty.exe.sha256`, the file size and the Windows `MZ` header before replacing the current executable, then restarts Painty. No installer is run during an update.

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+Z | Undo |
| Ctrl+Y or Ctrl+Shift+Z | Redo |
| Ctrl+O | Open a project |
| Ctrl+S | Save the current project |
| Ctrl+Shift+S | Export to PNG or JPEG |
| Ctrl+0 | Reset canvas zoom and position |

Use the **mouse wheel** to zoom around the pointer. **Middle-click and drag**, or **Space + drag**, to move the canvas. Right-click a palette color to edit it.

## Languages

The interface is available in English, French, Spanish, German and Italian. Choose a language in **Settings → Language**; it is saved with the other settings. Translations live in `assets/lang/` as JSON files, one per language. See [ARCHITECTURE.md](ARCHITECTURE.md#adding-a-language) to add another one.

## Build from source

Requirements:

- Windows 10 or 11
- [Rust](https://rustup.rs) (stable)
- Visual Studio Build Tools with the **C++ workload** (MSVC linker)
- [Inno Setup 6](https://jrsoftware.org/isinfo.php), only to build the installer

Run the application:

```powershell
.\run.ps1
```

Run the same checks as CI:

```powershell
.\check.ps1
```

Or run them directly:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Build the release executable, its checksum and the installer:

```powershell
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value

cargo build --release
New-Item dist -ItemType Directory -Force | Out-Null
Copy-Item target\release\painty.exe dist\painty.exe
$hash = (Get-FileHash dist\painty.exe -Algorithm SHA256).Hash.ToLower()
"$hash  painty.exe" | Set-Content -Path dist\painty.exe.sha256 -Encoding ascii
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" "/DMyAppVersion=$version" painty.iss
```

The artifacts are written to `dist\`: `painty.exe`, `painty.exe.sha256` and `PaintySetup-<version>.exe`.

## Releasing

1. Set the same version in `Cargo.toml` and `painty.iss`, then run `cargo build` to update `Cargo.lock`.
2. Write the release description in `RELEASE_NOTES.md`.
3. Copy it to `assets/archive/vX.Y.Z.md`. The app embeds the file matching its own version.
4. Commit, push, and wait for CI to pass.
5. Push the matching tag.

For a stable release:

```powershell
git tag vX.Y.Z
git push origin vX.Y.Z
```

For a beta release, use the same SemVer pre-release version everywhere (`Cargo.toml`, `painty.iss` and `assets/archive/`):

```powershell
git tag vX.Y.Z-beta.1
git push origin vX.Y.Z-beta.1
```

The `Release` workflow publishes stable tags as **Latest** and tags containing a hyphen as **Pre-release**. The tag must be exactly `v` followed by the version in `Cargo.toml`, and every release contains `painty.exe`, `painty.exe.sha256` and `PaintySetup-<version>.exe`.

## Settings

Settings are stored in `%APPDATA%\Painty\settings.txt`: theme, canvas color, language, automatic update check, update channel and palette.

## Project structure

See [ARCHITECTURE.md](ARCHITECTURE.md) for a description of every source file.

## License

Released under the [MIT License](LICENSE). © 2026 Stroke Works
