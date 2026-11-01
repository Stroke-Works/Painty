# Architecture

This document describes how Painty is organised, file by file.

## Overview

```text
main.rs ──► app.rs ──┬─► canvas.rs ──► model.rs
                    ├─► toolbar.rs ──► widgets.rs
                    ├─► settings.rs ─┐
                    ├─► dialogs ─────┴─► dialog.rs
                    ├─► export.rs
                    ├─► project.rs
                    ├─► updater.rs
                    ├─► i18n.rs ◄── assets/lang/*.json
                    └─► config.rs

theme.rs is used by every UI file.
```

## Folders

| Path | Content |
|------|---------|
| `src/` | Rust source code |
| `assets/` | Images, icons, flags and translations, embedded into the binary at compile time |
| `assets/svg/` | Icons (dark and light versions) |
| `assets/flags/` | Flags used by the language dropdown |
| `assets/fonts/` | Inter fonts (regular and semibold) |
| `assets/lang/` | Translations (`en.json`, `fr.json`, `es.json`, `de.json`, `it.json`) |
| `dist/` | Installer output (ignored by Git) |
| `target/` | Build output (ignored by Git) |

## Assets

| File | Use |
|------|-----|
| `icon.png` | Logo, window icon and README header (256 × 256 PNG) |
| `painty.ico` | Executable and installer icon (multi-size ICO) |
| `svg/settings_dark.svg` / `svg/settings_light.svg` | Settings icons for dark and light themes |
| `svg/undo_dark.svg` / `svg/undo_light.svg` | Undo icons for dark and light themes |
| `svg/trash_dark.svg` / `svg/trash_light.svg` | Clear icons for dark and light themes |
| `svg/brush_*.svg`, `eraser_*.svg`, `line_*.svg`, `rectangle_*.svg`, `ellipse_*.svg`, `eyedropper_*.svg` | Tool icons, with dark and light variants |
| `svg/export_dark.svg` / `svg/export_light.svg` | Export and project-menu icon |
| `flags/*.svg` | One flag per language (`en`, `fr`, `es`, `de`, `it`), used by the language dropdown |
| `fonts/Inter-Regular.ttf` / `fonts/Inter-SemiBold.ttf` | Interface fonts |
| `lang/*.json` | Interface translations, one file per language. Every file must contain the same keys and the same `{placeholders}` as `en.json`; a test checks it. |

Icons and flags are loaded using `egui::include_image!`, so they are embedded in the executable and do not need to be distributed separately.

## Source Files

| File | Role |
|------|------|
| `main.rs` | Entry point. Configures window options, the window icon, image loaders and fonts. Hides the console in release builds. |
| `app.rs` | Main application state (`Painty`). Handles keyboard shortcuts, project and export dialogs, the currently open dialog (`Modal`) and frame rendering. |
| `canvas.rs` | Drawing area. Smooths pointer movement, renders strokes and shapes, handles zoom/pan and color picking, erases strokes or shapes, and maintains undo/redo history. |
| `model.rs` | Core data types: `Tool`, `Geometry`, `Line` (a stroke or shape) and `Brush` (current settings). |
| `project.rs` | Reads and writes versioned `.painty` JSON projects, with limits on file size, strokes and points. |
| `toolbar.rs` | Floating toolbar at the bottom of the window. Its elements fold in and out with animations to fit the window width; it also contains tool, project and palette controls. |
| `widgets.rs` | Custom UI controls: text buttons, icon buttons, color swatches, size slider, segmented controls and the language dropdown. Focus rings are drawn only for keyboard navigation. |
| `dialog.rs` | Modal popup system: dimmed background, slide-in animation and scrolling when the window is too short. Popups are measured again once a window resize has settled. |
| `settings.rs` | Settings dialog: theme, canvas background, language, automatic update check, manual check and clickable version. |
| `export.rs` | Exports drawings to PNG or JPEG. Renders at up to 3× resolution and averages pixels for smoother edges. Very large drawings are exported at a lower scale. |
| `updater.rs` | Checks for updates and handles self-updates through GitHub Releases, including download progress and SHA-256 verification. Runs in background threads. |
| `i18n.rs` | Translation system. Defines `Lang` and `Lang::ALL`, loads the JSON files and provides `t("key")` for plain text and `tf("key", &[...])` for formatted text. Missing keys fall back to English. |
| `config.rs` | Loads and saves settings (theme, canvas background, language, automatic update check and palette) to `%APPDATA%\Painty\settings.txt`. |
| `theme.rs` | Defines light and dark theme colors (text contrast of at least 4.5:1), the Inter fonts and the egui style. |

## Toolbar

The toolbar is a single row that keeps the same order at every width. Each element sits in a cell whose width and opacity are animated, so it folds in or out instead of jumping:

1. The slider takes the space left over, between 32 and 120 px.
2. When it can reach 80 px with the text labels, Undo and Clear switch from icons to text.
3. When the slider can reach its full length, a palette row of six colors opens above the row.

Widths are measured every frame, so translations of any length fit. With the eraser active, the color button and the palette are dimmed and the size stays visible. The tool selector shows the active tool's SVG and includes the custom fill-shapes toggle. Right-clicking a swatch opens its color editor; the six colors are persisted in settings.

## Drawing

- **Brush:** Pointer positions are smoothed using `SMOOTHING` in `canvas.rs`. The smoothing is time-based, so it behaves consistently at any frame rate. Points are then added to a `Line`. `Line::push` skips points that are too close together, keeping long strokes lightweight. Each stroke is drawn as flat segments with round caps at both ends. A round disc is added only at the bends sharp enough for a flat joint to show a notch.
- **Eraser:** Each eraser step tests the eraser disc against the segments of every stroke, not only against their points, so a fast stroke with distant points is cut too. The parts outside the disc are kept as separate strokes. Nothing is allocated when the eraser touches nothing. A complete eraser gesture counts as one undo step.
- **Undo / Redo:** Each finished stroke or erase operation creates a checkpoint. Strokes are shared using `Arc`, so checkpoints only need to copy pointers. History is limited to `HISTORY_LIMIT` steps.
- **Shapes:** The line, rectangle and ellipse tools store their geometry in document coordinates. Rectangles and ellipses can be filled; exports render the same shape geometry independently of the current zoom.
- **Navigation and color picking:** The mouse wheel zooms around the pointer; middle-click and drag or Space+drag pans. Ctrl+0 resets the view. The eyedropper samples drawn strokes and filled shapes, then returns to the brush.

## Projects

The toolbar's project menu opens and saves editable `.painty` files; Ctrl+O and Ctrl+S provide the same actions. The installer registers Painty as an Open With option for `.painty` files, and `main.rs` passes a file opened by Windows to `project.rs` at startup. A project stores its versioned JSON document, drawing geometry, palette and current brush settings. Opening a project with an existing drawing asks for confirmation. Project files are limited to 64 MiB, 100,000 strokes and 2,000,000 points. PNG/JPEG export remains separate (Ctrl+Shift+S).

## Dialogs

`app.rs` maintains a single `Modal` value, which can represent the settings dialog, clear confirmation, project replacement confirmation, export form, operation error or quit confirmation.

The update prompt is driven by the updater status and waits until no other dialog is visible. Each dialog is rendered through `dialog.rs`, which preserves its content while it fades out.

Clicking the version in Settings opens the current `RELEASE_NOTES.md` in an in-app CommonMark viewer.

## Export

The export dialog asks for a file name and format. Once confirmed, a file dialog lets the user choose where to save the drawing. Use Ctrl+Shift+S; Ctrl+S is reserved for saving editable projects.

`export.rs` renders the entire drawing (not just the visible area) at up to three times the original resolution, averages each block of pixels and saves the result. The exported background uses the current canvas color.

The size is planned first (`plan()`): an image is limited to 64 megapixels, and the supersampling drops from 3 to 2 to 1 as the image grows, so a huge drawing is exported at a lower scale instead of failing or exhausting the memory.

## Update Flow

1. `fetch_latest()` retrieves the latest GitHub release and compares its tag with the version defined in `Cargo.toml`. Pre-releases are older than the final version.
2. If a newer version is available, it looks for the `painty.exe` and `painty.exe.sha256` release assets. A release without the checksum is refused, and both addresses must start with the repository's release URL.
3. When the user installs the update, the expected hash is read first, then the executable is downloaded next to the current one in chunks and hashed on the fly. `Progress` tracks the bytes received, and the UI reads this value every frame.
4. The download is rejected, and the file deleted, if it is incomplete, if its SHA-256 differs from the published one, or if it is not a Windows program (size limits, `MZ` header). Otherwise, the current executable is renamed and replaced with the downloaded version.
5. The new executable is launched, and the current application window closes.
6. On the next launch, `cleanup()` removes any leftover update files.

The checksum protects against a corrupted or truncated download. It does not protect against a compromised repository: that needs code signing.

## Data Flow (One Frame)

1. `app.rs` checks the updater status and reads the current theme from `theme.rs`.
2. Keyboard shortcuts are processed unless a dialog is open.
3. `canvas.rs` handles pointer input and renders the strokes.
4. `toolbar.rs` handles brush settings and returns any triggered actions.
5. Dialogs are rendered one at a time through `dialog.rs`.
6. When a setting changes, `config.rs` saves the updated configuration to disk.

## Build Files

| File | Role |
|------|------|
| `Cargo.toml` | Dependencies, application version and license |
| `build.rs` | Embeds `assets/painty.ico` into `painty.exe` when the target is Windows |
| `painty.iss` | Inno Setup configuration for building the installer. The version is passed with `/DMyAppVersion=`. |
| `run.ps1` | Sets the terminal tab title and runs `cargo run` |
| `release.ps1` | Builds the release executable, writes `painty.exe.sha256` and creates the installer, in `dist\` |
| `clippy.toml` | Clippy settings (argument limit of the widget helpers) |
| `.gitattributes` | Line endings: LF for source files, CRLF for PowerShell and Inno Setup scripts |
| `RELEASE_NOTES.md` | Description of the next release, read by the release workflow |
| `AGENT.md` | Guide for AI agents and chat assistants working on the project |
| `.github/workflows/rust-ci.yml` | On every push: formatting, Clippy and tests, on Windows |
| `.github/workflows/release.yml` | On a `v*` tag: tests, build, checksum, installer, and publication of the release |

## Adding a language

1. Add a variant to `Lang` in `i18n.rs`, and to `Lang::ALL`, `code()`, `name()` and `table()`.
2. Copy `assets/lang/en.json` to `assets/lang/<code>.json` and translate every value. Keep the `{placeholders}` unchanged.
3. Add `assets/flags/<code>.svg` and its line in `flag()` in `widgets.rs`.
4. Run `cargo test`: the tests fail if a key or a placeholder is missing or unknown.