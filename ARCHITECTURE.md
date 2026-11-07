# Architecture

This document describes how Painty is organized, file by file.

## Overview

```text
main.rs
  └─ app.rs
       ├─ canvas.rs ── model.rs, smooth.rs, geometry.rs, render.rs
       ├─ toolbar.rs ── widgets.rs, model.rs
       ├─ settings.rs ── widgets.rs, dialog.rs, updater.rs
       ├─ dialog.rs
       ├─ export.rs ── model.rs
       ├─ project.rs ── model.rs
       ├─ updater.rs
       ├─ i18n.rs ── assets/lang/*.json
       ├─ config.rs
       └─ titlebar.rs
theme.rs is used by every UI file.
```

The main window starts at 1200 × 800 px when the monitor allows it, is capped at 85 % of the monitor, and is centered on startup. It is borderless and opaque: a transparent window flickers while it is resized, so its corners are square.

## Folders

| Path | Content |
|------|---------|
| `src/` | Rust source code |
| `assets/` | Images, icons, fonts, flags, translations and archived release notes |
| `assets/archive/` | Release notes for each version, embedded in Settings |
| `assets/svg/` | Icons, with dark and light variants |
| `assets/flags/` | Flags used by the language dropdown |
| `assets/fonts/` | Inter Regular and SemiBold, plus their OFL license |
| `assets/lang/` | Translations (`en.json`, `fr.json`, `es.json`, `de.json`, `it.json`) |
| `dist/` | Installer output (ignored by Git) |
| `target/` | Build output (ignored by Git) |

## Assets

| File | Use |
|------|-----|
| `icon.png` | Logo, window icon and README header (256 × 256 PNG) |
| `painty.ico` | Executable and installer icon (multi-size ICO) |
| `archive/vX.Y.Z.md` | Release notes for one version, embedded with `include_str!` and shown in Settings |
| `svg/settings_dark.svg` / `svg/settings_light.svg` | Settings icons for dark and light themes |
| `svg/undo_dark.svg` / `svg/undo_light.svg` | Undo icons for dark and light themes |
| `svg/trash_dark.svg` / `svg/trash_light.svg` | Clear icons for dark and light themes |
| `svg/brush_*.svg`, `eraser_*.svg`, `line_*.svg`, `rectangle_*.svg`, `ellipse_*.svg`, `eyedropper_*.svg` | Tool icons, with dark and light variants |
| `svg/export_dark.svg` / `svg/export_light.svg` | Export and project-menu icon |
| `flags/*.svg` | One flag per language (`en`, `fr`, `es`, `de`, `it`) |
| `fonts/Inter-Regular.ttf` / `fonts/Inter-SemiBold.ttf` | Interface fonts |
| `lang/*.json` | Interface translations. Every file must contain the same keys and `{placeholders}` as `en.json`; a test checks it. |

Icons and flags are loaded with `egui::include_image!`, and fonts with `include_bytes!`, so they are embedded in the executable. Archived release notes are embedded at compile time; the build uses the archive file matching `CARGO_PKG_VERSION`.

## Source files

| File | Role |
|------|------|
| `main.rs` | Entry point. Configures the window, icon, image loaders and fonts, then starts the app. Hides the console in release builds. |
| `app.rs` | Main application state (`Painty`). Handles keyboard shortcuts, project and export actions, dialogs (`Modal`) and frame rendering. |
| `canvas.rs` | Drawing area. Handles pointer input, zoom and pan, color picking, the eraser, and the undo/redo history. |
| `smooth.rs` | Turns raw brush positions into a smooth polyline (quadratic curves between midpoints, without lag). |
| `geometry.rs` | Hit tests and the cuts made by the eraser. |
| `render.rs` | Builds each stroke's triangles once (round joints and caps, anti-aliased edges) and draws them. |
| `titlebar.rs` | Borderless window chrome: the floating minimize, maximize and close controls and the resize handles. The canvas fills the whole window. |
| `model.rs` | Core data types: `Tool`, `Geometry`, `Line` (a stroke or shape) and `Brush` (current settings). |
| `project.rs` | Reads and writes versioned `.painty` projects: compact binary format 2 with a SHA-256 checksum. Format 1 JSON projects are still read and are rewritten as format 2. |
| `toolbar.rs` | Floating toolbar at the bottom of the window. Its cells fold in and out with animations to fit the window width; it contains the tools, brush settings, project menu and palette. |
| `widgets.rs` | Custom UI controls: text buttons, icon buttons, color swatches, size slider, segmented controls and the language dropdown. Focus rings are drawn only for keyboard navigation. |
| `dialog.rs` | Modal popup system: dimmed background, fade and slide animation, and scrolling when the window is too short. |
| `settings.rs` | Settings dialog: theme, canvas color, language, automatic update check, update channel, manual check and version footer. |
| `export.rs` | Exports drawings to PNG or JPEG. Renders at up to 3× resolution and averages pixels for smoother edges. Very large drawings are exported at a lower scale. |
| `updater.rs` | Checks for updates and handles self-updates through GitHub Releases, including update channels, download progress and SHA-256 verification. Runs in background threads. |
| `i18n.rs` | Translation system. Defines `Lang` and `Lang::ALL`, loads the JSON files, and provides `t("key")` and `tf("key", &[...])`. Missing keys fall back to English. |
| `config.rs` | Loads and saves settings to `%APPDATA%\Painty\settings.txt`, including the beta channel and the palette. |
| `theme.rs` | Defines light and dark colors (text contrast of at least 4.5:1), the Inter fonts and the egui style. |

## Toolbar

The toolbar is a single row that keeps the same order at every width. Each element sits in a cell whose width and opacity are animated, so it folds in or out instead of jumping:

1. The brush-size slider takes the remaining space, between 32 and 120 px.
2. When the slider can reach 80 px with text labels, Undo and Clear switch from icons to text.
3. When the slider can reach its full length, a six-color palette row opens above the main row.

Widths are measured every frame, so translations of any length fit. With the eraser active, the color button and palette are dimmed while the size stays visible. The tool selector shows the active tool's SVG and includes the fill-shapes toggle. Right-clicking a swatch opens its color editor; the six colors are saved in settings.

## Drawing

- **Brush:** Pointer positions pass through `Smoother` (`smooth.rs`), which builds a smooth curve without delay. The stroke is stored as a `Line`, and `render.rs` builds its triangles once and reuses them each frame. Gentle bends share their corners; sharp bends and both ends get a round disc.
- **Eraser:** Each eraser step tests the eraser disc against every segment of every stroke, not only against their points, so a fast stroke with distant points is cut too. The parts outside the disc become separate strokes. Nothing is allocated when the eraser touches nothing. A complete gesture counts as one undo step.
- **Undo and redo:** Each finished stroke or erase operation creates a checkpoint. Strokes are shared with `Arc`, so a checkpoint only copies pointers. History is limited to 100 steps.
- **Shapes:** The line, rectangle and ellipse tools store their geometry in document coordinates. Rectangles and ellipses can be filled; export renders the same geometry independently of the current zoom.
- **Navigation and color picking:** The mouse wheel zooms around the pointer; middle-click and drag, or Space + drag, pans. Ctrl+0 resets the view. The eyedropper samples drawn strokes and filled shapes, then returns to the brush.

## Projects

The toolbar's project menu opens and saves editable `.painty` files; Ctrl+O and Ctrl+S provide the same actions. The installer registers Painty as an **Open with** option for `.painty` files, and `main.rs` passes a file opened by Windows to `project.rs` at startup.

A project stores its geometry, palette and current brush settings in a versioned binary file (format 2). Saving is atomic. Old JSON projects (format 1) still open and are rewritten in format 2 on the next save. Opening a project over an existing drawing asks for confirmation. Project files are limited to 64 MiB, 100,000 strokes and 2,000,000 points. PNG and JPEG export remains separate (Ctrl+Shift+S).

## Dialogs

`app.rs` maintains a single `Modal` value for the settings dialog, clear confirmation, project replacement confirmation, export form, operation error and quit confirmation.

The update prompt is driven by the updater status and waits until no other dialog is visible. Each dialog is rendered through `dialog.rs`, which preserves its content while it fades out.

Clicking the version in Settings opens the matching `assets/archive/vX.Y.Z.md` in a scrollable CommonMark viewer styled as part of the dialog card. The update prompt instead shows the notes returned by GitHub for the available release.

## Export

The export dialog asks for a file name and format. Once confirmed, a file dialog lets the user choose where to save the drawing. Ctrl+Shift+S opens it; Ctrl+S remains reserved for saving projects.

`export.rs` renders the entire drawing, not just the visible area, at up to three times the original resolution, averages each block of pixels and saves the result with a quality of 95 for JPEG. The background uses the current canvas color.

The output size is planned first (`plan()`): the image is limited to 64 megapixels, and supersampling drops from 3 to 2 to 1 as the working image approaches 120 megapixels. A huge drawing is therefore exported at a lower scale instead of failing or exhausting memory.

## Update flow

1. The stable channel requests the latest GitHub release (`releases/latest`). The beta channel requests the ten newest releases, including pre-releases, and picks the newest non-draft entry.
2. `fetch_latest()` compares the release tag with the version compiled into the app. SemVer ordering is used, and a pre-release is older than the final version with the same numbers.
3. If a newer version exists, the updater looks for the `painty.exe` and `painty.exe.sha256` assets. A release without the checksum is refused, and both download URLs must start with the repository's release prefix.
4. When the user installs the update, the expected hash is read first. The executable is then downloaded next to the current one in chunks and hashed while it arrives; `Progress` exposes the byte count to the UI.
5. The download is rejected and deleted if it is incomplete, if its SHA-256 differs from the published one, if its size is outside 512 KiB to 200 MiB, or if it lacks the `MZ` header. Otherwise the current executable is renamed and replaced.
6. The new executable is launched and the current window closes. On the next launch, `cleanup()` removes leftover update files.

The checksum protects against a corrupted or truncated download. It does not protect against a compromised repository; that requires code signing.

## Data flow (one frame)

1. `app.rs` centers the window on first launch, starts the update check, polls the updater and processes close requests.
2. It follows the system theme when no explicit theme was saved, then handles keyboard shortcuts unless a dialog is open.
3. `titlebar.rs` draws the floating window controls and resize handles.
4. The central panel fills the window with the canvas, and `canvas.rs` handles pointer input and draws the strokes.
5. `toolbar.rs` handles brush settings and returns any triggered action.
6. Settings, release notes and dialogs are rendered one at a time through `dialog.rs`.
7. When a setting changes, `config.rs` writes the updated configuration to disk.

## Build files

| File | Role |
|------|------|
| `Cargo.toml` | Dependencies, application version and license |
| `Cargo.lock` | Locked dependency versions, updated by Cargo |
| `build.rs` | Embeds `assets/painty.ico` into `painty.exe` when targeting Windows |
| `painty.iss` | Inno Setup configuration for the installer. The version is passed with `/DMyAppVersion=`. |
| `run.ps1` | Runs `cargo run` and sets the terminal tab title |
| `check.ps1` | Runs formatting, Clippy and tests, as CI does |
| `clean.ps1` | Runs `cargo clean` |
| `clippy.toml` | Clippy settings (argument limit of the widget helpers) |
| `.gitattributes` | Line endings: LF for source files, CRLF for PowerShell and Inno Setup files |
| `README.md` | User-facing overview, installation, build and release instructions |
| `AGENTS.md` | Guide for AI agents and chat assistants working on the project |
| `RELEASE_NOTES.md` | Description of the next release, used as the GitHub release body |
| `assets/archive/vX.Y.Z.md` | Archived release notes for that app version, embedded in Settings |
| `.github/workflows/rust-ci.yml` | On every push and pull request: formatting, Clippy and tests on Windows |
| `.github/workflows/release.yml` | On a `v*` tag: tests, build, checksum, installer and publication (Latest or Pre-release) |

## Adding a language

1. Add a variant to `Lang` in `i18n.rs`, and to `Lang::ALL`, `code()`, `name()` and `table()`.
2. Copy `assets/lang/en.json` to `assets/lang/<code>.json` and translate every value. Keep the `{placeholders}` unchanged.
3. Add `assets/flags/<code>.svg` and its line in `flag()` in `widgets.rs`.
4. Run `cargo test`: the tests fail if a key or a placeholder is missing or unknown.