# AGENT.md

Guide for AI agents and chat assistants working on Painty.

## Project

Painty is a minimal paint app for Windows, written in Rust with eframe/egui 0.29. The code is split into small files; [ARCHITECTURE.md](ARCHITECTURE.md) describes each one. The developer works on Windows with PowerShell.

## Before proposing a change

Run these three commands. The CI runs the same ones on the latest stable Rust, so a newer Clippy can report lints that an older toolchain does not.

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Rules

- Code, comments, commit messages and release notes are in English. The chat can be in French.
- Interface text goes in `assets/lang/*.json`, never in the code. Every language file must have the same keys and `{placeholders}` as `en.json`.
- Keep the updater strict: the SHA-256 check, the `MZ` and size checks, and the `DOWNLOAD_PREFIX` check in `updater.rs` must stay. Do not set `panic = "abort"`: the updater detects a worker thread that panicked.
- Do not enable ureq's `json` feature; responses are parsed with `serde_json`.
- Keep `Cargo.toml` and `painty.iss` on the same version. The tag of a release must be `v` followed by the version in `Cargo.toml`.
- Add or update a test with every fix.
- Do not add documentation or helper files that were not asked for.

## How to answer

- Be brief. Give only what changed.
- When changing a file, give the complete file, not a fragment, unless asked otherwise.
- `grep` and `gh` may not exist on the developer's machine: give PowerShell equivalents (`Select-String`, the GitHub website).
- Always give the git commands, with several small commits to keep a readable history. Commit messages are in English and start with a conventional prefix (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`, `ci:`).

## Releasing

1. Set the version in `Cargo.toml` and `painty.iss`, then run `cargo build` to update `Cargo.lock`.
2. Write `RELEASE_NOTES.md`. The release workflow fails without it.
3. Commit, push, wait for a green CI, then push the tag `vX.Y.Z`.
4. The release is titled `Painty X.Y.Z` and contains `painty.exe`, `painty.exe.sha256` and `PaintySetup-X.Y.Z.exe`.
