//! Update checks and self-update through GitHub Releases.
//! Only the program itself is downloaded (painty.exe), never an installer.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use eframe::egui::Context;

use crate::i18n::{t, tf};

/// Latest release of the repository. The release must contain an asset named `painty.exe`.
const API_URL: &str = "https://api.github.com/repos/Stroke-Works/painty/releases/latest";
const ASSET_NAME: &str = "painty.exe";
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

/// HTTP client with timeouts, so a silent server cannot block the updater forever.
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(IO_TIMEOUT)
        .timeout_write(IO_TIMEOUT)
        .build()
}

/// A newer version found on GitHub.
#[derive(Clone)]
pub struct Release {
    pub version: String,
    pub url: String,
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
            let result = download_and_replace(&ctx, &release.url, &progress);
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

    let url = json["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset["name"].as_str() == Some(ASSET_NAME))
        })
        .and_then(|asset| asset["browser_download_url"].as_str())
        .ok_or(t("error.no_asset"))?
        .to_owned();

    Ok(Some(Release { version, url }))
}

/// Compares "major.minor.patch" versions.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(v: &str) -> [u32; 3] {
        let mut parts = [0; 3];
        for (i, s) in v.split('.').take(3).enumerate() {
            parts[i] = s.parse().unwrap_or(0);
        }
        parts
    }
    parse(candidate) > parse(current)
}

/// Downloads the new executable, reporting progress, then swaps it with the running one.
/// Returns the path of the replaced executable, to restart it.
fn download_and_replace(ctx: &Context, url: &str, progress: &Progress) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or(t("error.install_folder"))?.to_path_buf();
    let new_exe = dir.join(NEW_NAME);
    let old_exe = dir.join(OLD_NAME);

    // 1. Download the new executable next to the current one.
    let response = agent().get(url).call().map_err(|e| e.to_string())?;
    // Size of the file, if the server tells us. Without it the bar is indeterminate.
    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    progress.total.store(total, Ordering::Relaxed);

    let mut reader = response.into_reader();
    let mut file = File::create(&new_exe).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress.done.store(done, Ordering::Relaxed);
        ctx.request_repaint();
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    // Refuse a truncated download: replacing the executable with it would break
    // the installation.
    if total > 0 && done != total {
        let _ = std::fs::remove_file(&new_exe);
        return Err(tf(
            "error.incomplete",
            &[("done", done.to_string().as_str()), ("total", total.to_string().as_str())],
        ));
    }

    // 2. Windows allows renaming a running exe, but not overwriting it.
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
}