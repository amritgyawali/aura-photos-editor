//! Limits, and the watchdog that enforces them.
//!
//! One file must never be able to stop a wedding. Two things can go wrong inside
//! a decoder that no amount of careful parsing prevents: it can take
//! unboundedly long on a pathological input, and it can be asked to allocate
//! more memory than the machine has. Both are handled here, before the decoder
//! runs, and both end as a Problems-list row rather than as a hung application.
//!
//! **Time.** Work runs on a worker thread and the caller waits with a deadline.
//! If the deadline passes, the caller gives up and returns `AURA-RAW-2004`; the
//! worker is left to finish and its result is dropped, because killing a thread
//! mid-allocation is not something a safe program can do.
//!
//! **Memory.** Dimensions are multiplied and checked against a ceiling *before*
//! any buffer is created, so a header claiming a billion pixels costs one
//! multiplication rather than a terabyte.
//!
//! Note on panics: the release profile aborts rather than unwinds, so this
//! watchdog isolates hangs and errors but cannot catch a panic in release. That
//! is why the decode path contains no `unsafe`, no `unwrap` and no unchecked
//! indexing, and why every container parser has a fuzz test.

use std::sync::mpsc;
use std::time::Duration;

use aura_core::clock::Clock;
use aura_core::errors::raw::{timed_out, worker_lost};
use aura_core::AuraResult;

/// What a single decode is allowed to consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    /// Wall-clock deadline for one file at one tier.
    pub timeout_ms: u64,
    /// Largest image, in pixels, we will allocate for.
    pub max_pixels: u64,
    /// Largest single allocation, in bytes.
    pub max_alloc_bytes: u64,
}

impl DecodeLimits {
    /// Tier 1: copying an embedded JPEG is milliseconds of work, so a stall
    /// here is a storage problem and five seconds is already generous.
    #[must_use]
    pub const fn tier1() -> Self {
        Self {
            timeout_ms: 5_000,
            max_pixels: 400_000_000,
            max_alloc_bytes: 512 * 1024 * 1024,
        }
    }

    /// Tier 2: a half-size demosaic plus a resize, budgeted at 120 ms, with a
    /// deadline an order of magnitude above the budget so that a slow machine
    /// is slow rather than broken.
    #[must_use]
    pub const fn tier2() -> Self {
        Self {
            timeout_ms: 10_000,
            max_pixels: 400_000_000,
            max_alloc_bytes: 1024 * 1024 * 1024,
        }
    }

    /// Tier 3: full sensor resolution, tiled. Budgeted at 1.8 s for 45 MP.
    #[must_use]
    pub const fn tier3() -> Self {
        Self {
            timeout_ms: 20_000,
            max_pixels: 400_000_000,
            max_alloc_bytes: 2048 * 1024 * 1024,
        }
    }
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self::tier2()
    }
}

/// Run `work` on a worker thread and give up after `limits.timeout_ms`.
///
/// # Errors
///
/// `AURA-RAW-2004` when the deadline passes, `AURA-RAW-2008` when the worker
/// ends without sending a result, or whatever `work` itself returned.
pub fn guarded<T, F>(stage: &'static str, limits: DecodeLimits, work: F) -> AuraResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AuraResult<T> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name(format!("aura-decode-{stage}"))
        .spawn(move || {
            // A closed channel means the caller already timed out. Dropping the
            // result is the correct outcome, not an error.
            let _ = sender.send(work());
        });

    if spawned.is_err() {
        return Err(worker_lost(stage));
    }

    match receiver.recv_timeout(Duration::from_millis(limits.timeout_ms)) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(timed_out(stage, limits.timeout_ms)),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(worker_lost(stage)),
    }
}

/// Time one piece of work through the sanctioned clock.
///
/// Returns the result and the elapsed milliseconds, saturated into the `u32`
/// that [`crate::contract::pixels::PixelBuffer::decode_ms`] carries.
pub fn timed<T>(clock: &dyn Clock, work: impl FnOnce() -> T) -> (T, u32) {
    let started = clock.monotonic_ms();
    let value = work();
    let elapsed = clock.monotonic_ms().saturating_sub(started);
    (value, u32::try_from(elapsed).unwrap_or(u32::MAX))
}

/// Refuse a decode whose declared size exceeds the ceiling, before allocating.
///
/// # Errors
///
/// `AURA-RAW-2005` when `width * height` is above `limits.max_pixels`, or when
/// the buffer those dimensions imply is above `limits.max_alloc_bytes`.
pub fn check_dimensions(
    width: u32,
    height: u32,
    bytes_per_pixel: u64,
    limits: DecodeLimits,
) -> AuraResult<()> {
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.max_pixels {
        return Err(aura_core::errors::raw::too_large(pixels, limits.max_pixels));
    }
    let bytes = pixels.saturating_mul(bytes_per_pixel);
    if bytes > limits.max_alloc_bytes {
        return Err(aura_core::errors::raw::too_large(
            bytes,
            limits.max_alloc_bytes,
        ));
    }
    Ok(())
}
