# Agent guide

Guide for AI agents and chat assistants working on Painty.

## Project

Painty is a minimal paint application for Windows, written in Rust with eframe/egui 0.29. The code is split into small modules; [ARCHITECTURE.md](ARCHITECTURE.md) describes each one. The canonical repository is [Stroke-Works/Painty](https://github.com/Stroke-Works/Painty). The developer works on Windows with PowerShell.

## Before proposing a change

Run these commands. CI runs the same checks on the latest stable Rust, so a newer Clippy can report lints that an older toolchain does not.

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`.\check.ps1` runs the same three commands.

## Rules

- Keep code, comments, commit messages and release notes consistent with the project's conventions.
- Interface text goes in `assets/lang/*.json`, never in the code. Every language file must have the same keys and `{placeholders}` as `en.json`.
- Keep the updater strict: the SHA-256 check, the `MZ` and size checks, and the `DOWNLOAD_PREFIX` check in `updater.rs` must stay. Keep the API URLs, `DOWNLOAD_PREFIX` and their tests on the canonical `Stroke-Works/Painty` repository. Do not set `panic = "abort"`: the updater detects a worker thread that panicked.
- Do not enable ureq's `json` feature; responses are parsed with `serde_json`.
- Keep `Cargo.toml` and `painty.iss` on the same version. A release tag is exactly `v` followed by that version.
- Add or update a test with every fix.
- Do not add documentation or helper files that were not asked for.

## How to answer

- Be brief. Give only what changed.
- When changing a file, give the complete file, not a fragment, unless asked otherwise.
- `grep` and `gh` may not exist on the developer's machine: give PowerShell equivalents (`Select-String`, the GitHub website).
- Always give the git commands, with several small commits to keep a readable history. Commit messages use a conventional prefix (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`, `ci:`).

## Releasing

1. Set the same version in `Cargo.toml` and `painty.iss`, then run `cargo build` to update `Cargo.lock`.
2. Write `RELEASE_NOTES.md`; this becomes the GitHub release description.
3. Copy the same notes to `assets/archive/vX.Y.Z.md`, replacing `X.Y.Z` with the new version. Settings embeds this version-specific file, so it must exist before the build.
4. Commit, push, wait for a green CI, then push the tag `vX.Y.Z`.
5. The `Release` workflow publishes stable tags as **Latest**. A tag containing a hyphen, such as `vX.Y.Z-beta.1`, is published as **Pre-release** and does not replace the latest stable release. The version in `Cargo.toml` must already include the same suffix.
6. A release contains `painty.exe`, `painty.exe.sha256` and `PaintySetup-X.Y.Z.exe`.
