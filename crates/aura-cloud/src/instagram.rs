//! Explicit, inbound-only public Instagram reference retrieval through Instaloader.
//! User photographs and credentials are never passed to the helper.
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use aura_core::{clock::Clock, progress::CancelToken, AuraResult};
use serde::{Deserialize, Serialize};

/// Actual retrieval coverage, including partial or blocked access.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchReport {
    pub folder: String,
    pub fetched: u32,
    pub skipped: u32,
    pub complete: bool,
    pub message: String,
}

/// Fetch public still photographs, without login or automatic retries.
///
/// # Errors
/// Invalid handles, missing Python, cancellation, timeout, or invalid helper output.
pub fn fetch(
    handle: &str,
    folder: &Path,
    limit: u32,
    cancel: &CancelToken,
    clock: &dyn Clock,
) -> AuraResult<FetchReport> {
    fn refused(message: impl Into<String>) -> aura_core::AuraError {
        let message = message.into();
        let mut error = aura_core::errors::ml::look_reference_refused(message.clone());
        error.user_message = message;
        error
    }
    if handle.is_empty()
        || handle.len() > 30
        || !handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
    {
        return Err(refused("Enter a valid Instagram profile handle"));
    }
    let mut command = Command::new("python");
    command
        .arg("-c")
        .arg(include_str!("instagram_fetch.py"))
        .arg(handle)
        .arg(folder)
        .arg(limit.clamp(8, 2000).to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|_| refused("Instagram retrieval needs Python with Instaloader. You can also choose saved reference photos."))?;
    let started = clock.monotonic_ms();
    loop {
        if cancel.is_cancelled() || clock.monotonic_ms().saturating_sub(started) > 600_000 {
            let _ = child.kill();
            let _ = child.wait();
            return Err(refused("Instagram retrieval stopped. Try a smaller photo limit or use saved reference photos."));
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(refused(format!(
                    "Cannot read Instagram retrieval status: {error}"
                )));
            }
        }
    }
    let mut output = String::new();
    if let Some(stdout) = child.stdout.take() {
        stdout
            .take(16_384)
            .read_to_string(&mut output)
            .map_err(|e| refused(e.to_string()))?;
    }
    serde_json::from_str(&output).map_err(|_| {
        refused("Instagram retrieval did not return a result. Try saved reference photos instead.")
    })
}
