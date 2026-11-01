<p align="center">
  <img src="assets/icon.png" width="128" alt="Painty logo">
</p>

<h1 align="center">Painty</h1>

<p align="center">
  A minimal paint app for Windows, written in Rust with egui.
</p>

<p align="center">
  <a href="https://github.com/Gtisseran/painty/releases/latest"><b>Download</b></a>
  ·
  <a href="ARCHITECTURE.md">Architecture</a>
  ·
  <a href="LICENSE">MIT License</a>
</p>

---

## Features

- **Brush** with adjustable size and color, and a color picker
- **Smooth strokes** that follow the pointer without jitter
- **Eraser** that cuts through existing strokes
- **Line, rectangle and ellipse tools**, with optional shape filling
- **Eyedropper** for picking colors from the drawing
- **Editable projects** saved as `.painty` files, including the drawing, brush and palette
- **Canvas navigation** with zoom and pan
- **Customizable six-color palette**, saved between sessions
- **Undo and redo** (Ctrl+Z / Ctrl+Y or Ctrl+Shift+Z)
- **Export** to PNG or JPEG, with a name and a format (Ctrl+Shift+S)
- **Clear all**, with confirmation
- **Light and dark theme**, with a light or dark canvas chosen separately
- **Five languages**: English, French, Spanish, German and Italian, switchable in Settings
- **Responsive toolbar** whose elements fold in and out smoothly as the window is resized
- **Automatic updates** from GitHub Releases, checked against a SHA-256 before they are applied, with a progress bar
- **Settings saved** between sessions

## Install

1. Go to the [latest release](https://github.com/Gtisseran/painty/releases/latest).
2. Download `PaintySetup-x.y.z.exe` and run it.
3. Choose where to install. No administrator rights are needed.

Painty checks for updates when it starts (this can be turned off in **Settings**). You can also check by hand from **Settings → Check for updates**.
After installing, Windows can offer Painty in **Open with** for `.painty` project files.
Click the version in **Settings** to read the current release notes.

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+Z | Undo |
| Ctrl+Y or Ctrl+Shift+Z | Redo |
| Ctrl+O | Open a project |
| Ctrl+S | Save the current project |
| Ctrl+Shift+S | Export to PNG or JPEG |
| Ctrl+0 | Reset canvas zoom and position |

Use the **mouse wheel** to zoom in or out around the pointer. **Middle-click and drag** to move the canvas; Space + drag is also supported. Right-click a palette color to edit it.

## Languages

The interface is available in English, French, Spanish, German and Italian. The language is chosen from a drop-down in **Settings → Language** and saved with the other settings. Translations are stored in `assets/lang/` as JSON files, one per language. To add a language, see [ARCHITECTURE.md](ARCHITECTURE.md#adding-a-language).

## Build from source

Requirements:

- Windows 10 or 11
- [Rust](https://rustup.rs) (stable)
- Visual Studio Build Tools with the **C++ workload** (MSVC linker)
- [Inno Setup 6](https://jrsoftware.org/isinfo.php), only to build the installer

Run the app:

```powershell
.\run.ps1
```

Check the code the way the CI does:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Build the executable, its checksum and the installer:

```powershell
.\release.ps1
```

Everything is written to `dist\`: `painty.exe`, `painty.exe.sha256` and `PaintySetup-<version>.exe`.

## Updates

Painty downloads only `painty.exe` from the latest GitHub release and replaces itself. The download is checked against `painty.exe.sha256`, published with the release, and is refused if it does not match. No installer is run, and the new version starts right after the download.

## Releasing

1. Set the new version in `Cargo.toml`, then run `cargo build` to update `Cargo.lock`.
2. Write the release notes in `RELEASE_NOTES.md`.
3. Copy those notes to `assets/archive/vX.Y.Z.md`, replacing `X.Y.Z` with the new version. This keeps the notes shown in Settings tied to the installed version.
4. Commit and push, wait for the CI to pass, then push the tag:

```powershell
git tag v0.2.3
git push origin v0.2.3
```

The `Release` workflow builds `painty.exe`, its `.sha256` and the installer, and publishes them as **Painty 0.2.3**. The tag must match the version in `Cargo.toml`.

## Settings

Settings (theme, canvas color, language, update check) are stored in `%APPDATA%\Painty\settings.txt`.

## Project structure

See [ARCHITECTURE.md](ARCHITECTURE.md) for a full description of each file.

## License

Released under the [MIT License](LICENSE). © 2026 Gtisseran