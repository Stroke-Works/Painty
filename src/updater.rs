//! Update checks and self-update through GitHub Releases.
//! Only the program itself is downloaded (painty.exe), never an installer.
//! The download is checked against the SHA-256 published with the release
//! (`painty.exe.sha256`) before it replaces the running executable.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use eframe::egui::Context;
use sha2::{Digest, Sha256};

use crate::i18n::{t, tf};

/// Latest release of the repository. The release must contain an asset named `painty.exe`
/// and its checksum, `painty.exe.sha256`.
const API_URL: &str = "https://api.github.com/repos/Stroke-Works/Painty/releases/latest";
const ASSET_NAME: &str = "painty.exe";
const CHECKSUM_ASSET_NAME: &str = "painty.exe.sha256";
/// Downloads are only accepted from the releases of this repository.
const DOWNLOAD_PREFIX: &str = "https://github.com/Stroke-Works/Painty/releases/download/";
/// Name given to the previous executable after replacement.
const OLD_NAME: &str = "painty.old.exe";
/// Name of the downloaded executable before it replaces the current one.
const NEW_NAME: &str = "painty.new.exe";
/// Size of each read while downloading. Smaller reads update the progress bar more often.
const CHUNK_SIZE: usize = 64 * 1024;
/// Maximum time to wait for the connection to GitHub to open.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum time to wait for data while checking or downloading.
const IO_TIMEOUT: Duration = Duration::from_secs(30);
/// A real Painty build is far bigger than this; anything smaller is not the program.
const MIN_EXE_SIZE: u64 = 512 * 1024;
/// Upper bound for the download, so a wrong file cannot fill the disk.
const MAX_EXE_SIZE: u64 = 200 * 1024 * 1024;
/// The checksum file is a single line; never read more than this.
const MAX_CHECKSUM_FILE: u64 = 1024;

/// HTTP client with timeouts, so a silent server cannot block the updater forever.
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .https_only(true)
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(IO_TIMEOUT)
        .timeout_write(IO_TIMEOUT)
        .build()
}

/// A newer version found on GitHub.
#[derive(Clone)]
pub struct Release {
    pub version: String,
    /// Download address of `painty.exe`.
    pub url: String,
    /// Download address of the SHA-256 of `painty.exe`.
    pub sha256_url: String,
}

/// Download progress, shared between the download thread and the UI.
#[derive(Default)]
pub struct Progress {
    done: AtomicU64,
    total: AtomicU64,
}

impl Progress {
    /// Fraction downloaded, from 0.0 to 1.0. `None` if the total size is unknown.
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return None;
        }
        let done = self.done.load(Ordering::Relaxed);
        Some((done as f32 / total as f32).clamp(0.0, 1.0))
    }
}

/// What the updater is doing, read by the UI.
#[derive(Clone, Default)]
pub enum Status {
    #[default]
    Idle,
    /// Asking GitHub for the latest version.
    Checking,
    UpToDate,
    /// A newer version exists and has not been installed yet.
    Available(Release),
    /// Downloading the new executable, with its progress.
    Downloading(Arc<Progress>),
    /// The last manual check failed (shown in settings only).
    CheckFailed(String),
    /// The download or replacement failed (shown in the prompt).
    InstallFailed(String),
}

/// Messages sent back by background threads.
enum Message {
    Checked {
        result: Result<Option<Release>, String>,
        /// Manual checks show errors; automatic checks stay silent.
        manual: bool,
    },
    /// Holds the path of the executable that was replaced.
    Installed(Result<PathBuf, String>),
}

#[derive(Default)]
pub struct Updater {
    rx: Option<Receiver<Message>>,
    pub status: Status,
    /// Set after a successful install: the app must close so the new version starts.
    pub restart: bool,
}

impl Updater {
    /// Starts a version check in the background.
    pub fn check(&mut self, ctx: &Context, manual: bool) {
        if matches!(self.status, Status::Checking | Status::Downloading(_)) {
            return;
        }
        self.status = Status::Checking;

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = fetch_latest();
            let _ = tx.send(Message::Checked { result, manual });
            ctx.request_repaint();
        });
    }

    /// Downloads the new executable in the background, then replaces the current one.
    pub fn install(&mut self, ctx: &Context, release: Release) {
        if matches!(self.status, Status::Downloading(_)) {
            return;
        }
        let progress = Arc::new(Progress::default());
        self.status = Status::Downloading(progress.clone());

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = download_and_replace(&ctx, &release, &progress);
            let _ = tx.send(Message::Installed(result));
            ctx.request_repaint();
        });
    }

    /// Closes the update prompt without installing.
    pub fn dismiss(&mut self) {
        if matches!(self.status, Status::Available(_) | Status::InstallFailed(_)) {
            self.status = Status::Idle;
        }
    }

    /// Handles messages from background threads. Call once per frame.
    pub fn poll(&mut self, ctx: &Context) {
        let Some(rx) = &self.rx else {
            return;
        };
        // Wake up regularly, so a thread that died without reporting is noticed
        // even when nothing else asks for repaints.
        ctx.request_repaint_after(Duration::from_millis(100));
        let message = match rx.try_recv() {
            Ok(message) => message,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                // The thread ended without sending a result: never leave the UI
                // stuck on "Checking" or "Downloading".
                if matches!(self.status, Status::Checking) {
                    self.status = Status::CheckFailed(t("error.check_interrupted").to_owned());
                } else if matches!(self.status, Status::Downloading(_)) {
                    self.status = Status::InstallFailed(t("error.download_interrupted").to_owned());
                }
                return;
            }
        };
        self.rx = None;

        match message {
            Message::Checked { result, manual } => {
                self.status = match result {
                    Ok(Some(release)) => Status::Available(release),
                    Ok(None) => Status::UpToDate,
                    Err(error) if manual => Status::CheckFailed(error),
                    Err(_) => Status::Idle,
                };
            }
            Message::Installed(Ok(path)) => {
                // Only ask to close once the new process has actually started.
                if let Err(error) = restart(&path) {
                    self.status = Status::InstallFailed(error);
                } else {
                    self.restart = true;
                }
            }
            Message::Installed(Err(error)) => {
                self.status = Status::InstallFailed(error);
            }
        }
    }
}

/// Asks GitHub for the latest release. Returns `None` if this version is up to date.
fn fetch_latest() -> Result<Option<Release>, String> {
    // Read the body as text, then parse it with serde_json (ureq's `json` feature is not enabled).
    let body = agent()
        .get(API_URL)
        .set("User-Agent", "painty-updater")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;

    let tag = json["tag_name"].as_str().ok_or(t("error.no_tag"))?;
    let version = tag.trim_start_matches('v').to_owned();
    if !is_newer(&version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let url = asset_url(&json, ASSET_NAME).ok_or(t("error.no_asset"))?;
    let sha256_url = asset_url(&json, CHECKSUM_ASSET_NAME).ok_or(t("error.no_checksum"))?;
    // Never follow an address that does not come from this repository's releases.
    if !is_allowed_download_url(&url) || !is_allowed_download_url(&sha256_url) {
        return Err(t("error.bad_url").to_owned());
    }

    Ok(Some(Release {
        version,
        url,
        sha256_url,
    }))
}

fn is_allowed_download_url(url: &str) -> bool {
    url.starts_with(DOWNLOAD_PREFIX)
}

/// Download address of the release asset called `name`.
fn asset_url(release: &serde_json::Value, name: &str) -> Option<String> {
    release["assets"]
        .as_array()?
        .iter()
        .find(|asset| asset["name"].as_str() == Some(name))?["browser_download_url"]
        .as_str()
        .map(str::to_owned)
}

/// A version split into its numbers and its optional pre-release tag (`1.2.3-beta`).
fn parse_version(v: &str) -> ([u32; 3], Option<&str>) {
    let v = v.trim().trim_start_matches('v');
    // Build metadata (`+...`) never affects the order.
    let v = v.split('+').next().unwrap_or(v);
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let mut parts = [0; 3];
    for (i, s) in core.split('.').take(3).enumerate() {
        parts[i] = s.parse().unwrap_or(0);
    }
    (parts, pre)
}

/// Compares "major.minor.patch" versions. A pre-release (`0.3.0-beta`) is older than
/// the same version without a tag (`0.3.0`).
fn is_newer(candidate: &str, current: &str) -> bool {
    let (candidate, candidate_pre) = parse_version(candidate);
    let (current, current_pre) = parse_version(current);
    match candidate.cmp(&current) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (candidate_pre, current_pre) {
            (None, Some(_)) => true,
            (Some(a), Some(b)) => a > b,
            _ => false,
        },
    }
}

/// Reads the SHA-256 published next to the release. The file holds the hash,
/// optionally followed by a file name (the output of `sha256sum`).
fn fetch_checksum(url: &str) -> Result<String, String> {
    let mut text = String::new();
    agent()
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_reader()
        .take(MAX_CHECKSUM_FILE)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    parse_checksum(&text).ok_or_else(|| t("error.bad_checksum_file").to_owned())
}

/// First word of the file, if it is a SHA-256 in hexadecimal. Returned in lowercase.
fn parse_checksum(text: &str) -> Option<String> {
    let word = text
        .split_whitespace()
        .next()?
        .trim_start_matches('\u{feff}');
    let valid = word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit());
    valid.then(|| word.to_ascii_lowercase())
}

/// What was downloaded: its size and its SHA-256, in lowercase hexadecimal.
struct Downloaded {
    size: u64,
    sha256: String,
}

/// Downloads `url` into `dest`, reporting progress and hashing on the fly.
fn download(
    ctx: &Context,
    url: &str,
    dest: &Path,
    progress: &Progress,
) -> Result<Downloaded, String> {
    let response = agent().get(url).call().map_err(|e| e.to_string())?;
    // Size of the file, if the server tells us. Without it the bar is indeterminate.
    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    progress.total.store(total, Ordering::Relaxed);

    let mut reader = response.into_reader();
    let mut file = File::create(dest).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        done += n as u64;
        if done > MAX_EXE_SIZE {
            return Err(t("error.bad_exe").to_owned());
        }
        progress.done.store(done, Ordering::Relaxed);
        ctx.request_repaint();
    }
    file.flush().map_err(|e| e.to_string())?;

    // Refuse a truncated download: replacing the executable with it would break
    // the installation.
    if total > 0 && done != total {
        return Err(tf(
            "error.incomplete",
            &[
                ("done", done.to_string().as_str()),
                ("total", total.to_string().as_str()),
            ],
        ));
    }

    Ok(Downloaded {
        size: done,
        sha256: format!("{:x}", hasher.finalize()),
    })
}

/// Checks the downloaded file: the hash must match the published one, and the file
/// must look like a Windows program.
fn verify(path: &Path, downloaded: &Downloaded, expected_sha256: &str) -> Result<(), String> {
    if downloaded.sha256 != expected_sha256 {
        return Err(t("error.checksum_mismatch").to_owned());
    }
    if downloaded.size < MIN_EXE_SIZE || !has_exe_header(path) {
        return Err(t("error.bad_exe").to_owned());
    }
    Ok(())
}

/// True when the file starts with the "MZ" mark of Windows executables.
fn has_exe_header(path: &Path) -> bool {
    let mut header = [0u8; 2];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && &header == b"MZ"
}

/// Downloads the new executable, checks it, then swaps it with the running one.
/// Returns the path of the replaced executable, to restart it.
fn download_and_replace(
    ctx: &Context,
    release: &Release,
    progress: &Progress,
) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or(t("error.install_folder"))?.to_path_buf();
    let new_exe = dir.join(NEW_NAME);
    let old_exe = dir.join(OLD_NAME);

    // 1. Read the expected hash first: without it nothing is downloaded.
    let expected = fetch_checksum(&release.sha256_url)?;

    // 2. Download the new executable next to the current one, and check it.
    // Whatever goes wrong, the unusable file is removed.
    let checked = download(ctx, &release.url, &new_exe, progress)
        .and_then(|downloaded| verify(&new_exe, &downloaded, &expected));
    if let Err(error) = checked {
        let _ = std::fs::remove_file(&new_exe);
        return Err(error);
    }

    // 3. Windows allows renaming a running exe, but not overwriting it.
    let _ = std::fs::remove_file(&old_exe);
    std::fs::rename(&exe, &old_exe).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&new_exe, &exe) {
        // Put the old executable back so the app keeps working.
        let _ = std::fs::rename(&old_exe, &exe);
        return Err(e.to_string());
    }

    Ok(exe)
}

/// Starts the replaced executable. The current process closes right after.
fn restart(path: &Path) -> Result<(), String> {
    std::process::Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| tf("error.restart", &[("error", e.to_string().as_str())]))
}

/// Removes files left over from an update or a failed download. Call at startup.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = std::fs::remove_file(dir.join(OLD_NAME));
            let _ = std::fs::remove_file(dir.join(NEW_NAME));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_worker_does_not_leave_updater_busy() {
        let ctx = Context::default();

        let (tx, rx) = channel();
        drop(tx);
        let mut updater = Updater {
            rx: Some(rx),
            status: Status::Checking,
            restart: false,
        };
        updater.poll(&ctx);
        assert!(matches!(updater.status, Status::CheckFailed(_)));

        let (tx, rx) = channel();
        drop(tx);
        updater.rx = Some(rx);
        updater.status = Status::Downloading(Arc::new(Progress::default()));
        updater.poll(&ctx);
        assert!(matches!(updater.status, Status::InstallFailed(_)));
    }

    #[test]
    fn versions_are_compared_by_number_not_by_text() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.2.2", "0.2.1"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
    }

    #[test]
    fn prerelease_is_older_than_the_final_version() {
        assert!(is_newer("0.3.0", "0.3.0-beta"));
        assert!(!is_newer("0.3.0-beta", "0.3.0"));
        assert!(is_newer("0.3.0-beta.2", "0.3.0-beta.1"));
        assert!(is_newer("0.3.0-beta", "0.2.9"));
        assert!(!is_newer("0.3.0-beta", "0.3.0-beta"));
    }

    #[test]
    fn version_prefix_and_garbage_are_tolerated() {
        assert!(is_newer("v0.3.0", "0.2.1"));
        assert!(!is_newer("nonsense", "0.2.1"));
        assert!(!is_newer("0.2.1+build5", "0.2.1"));
    }

    #[test]
    fn checksum_file_formats_are_understood() {
        let hash = "a".repeat(64);
        assert_eq!(parse_checksum(&hash), Some(hash.clone()));
        // Output of `sha256sum`, with a file name and a line break.
        assert_eq!(
            parse_checksum(&format!("{hash}  painty.exe\r\n")),
            Some(hash.clone())
        );
        // Upper case (PowerShell's Get-FileHash) and a byte order mark.
        assert_eq!(
            parse_checksum(&format!("\u{feff}{}\n", hash.to_uppercase())),
            Some(hash)
        );
        assert_eq!(parse_checksum(""), None);
        assert_eq!(parse_checksum("not a hash"), None);
        assert_eq!(parse_checksum(&"g".repeat(64)), None);
        assert_eq!(parse_checksum(&"a".repeat(63)), None);
    }

    #[test]
    fn release_assets_are_found_by_name() {
        let release = serde_json::json!({
            "assets": [
                { "name": "other.zip", "browser_download_url": "https://example.com/other.zip" },
                { "name": "painty.exe", "browser_download_url": "https://example.com/painty.exe" }
            ]
        });
        assert_eq!(
            asset_url(&release, "painty.exe").as_deref(),
            Some("https://example.com/painty.exe")
        );
        assert_eq!(asset_url(&release, "painty.exe.sha256"), None);
        assert_eq!(asset_url(&serde_json::json!({}), "painty.exe"), None);
    }

    #[test]
    fn release_download_urls_match_the_canonical_repository_name() {
        assert!(is_allowed_download_url(
            "https://github.com/Stroke-Works/Painty/releases/download/v0.2.3/painty.exe"
        ));
        assert!(!is_allowed_download_url(
            "https://github.com/Stroke-Works/painty/releases/download/v0.2.3/painty.exe"
        ));
        assert!(!is_allowed_download_url(
            "https://example.com/Stroke-Works/Painty/releases/download/v0.2.3/painty.exe"
        ));
    }

    /// Writes a fake executable of `size` bytes with the given first bytes.
    fn temp_file(name: &str, start: &[u8], size: usize) -> (PathBuf, Downloaded) {
        let path = std::env::temp_dir().join(format!("painty_test_{}_{name}", std::process::id()));
        let mut bytes = vec![0u8; size];
        bytes[..start.len()].copy_from_slice(start);
        std::fs::write(&path, &bytes).unwrap();
        let downloaded = Downloaded {
            size: size as u64,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        };
        (path, downloaded)
    }

    #[test]
    fn a_matching_executable_is_accepted() {
        let (path, downloaded) = temp_file("ok.exe", b"MZ", MIN_EXE_SIZE as usize);
        let result = verify(&path, &downloaded, &downloaded.sha256.clone());
        let _ = std::fs::remove_file(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn a_different_hash_is_refused() {
        let (path, downloaded) = temp_file("hash.exe", b"MZ", MIN_EXE_SIZE as usize);
        let result = verify(&path, &downloaded, &"0".repeat(64));
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn a_file_that_is_not_a_program_is_refused() {
        // Right hash, but no "MZ" mark.
        let (path, downloaded) = temp_file("page.exe", b"<html>", MIN_EXE_SIZE as usize);
        let result = verify(&path, &downloaded, &downloaded.sha256.clone());
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());

        // Right hash and mark, but far too small.
        let (path, downloaded) = temp_file("tiny.exe", b"MZ", 1024);
        let result = verify(&path, &downloaded, &downloaded.sha256.clone());
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
    }
}
