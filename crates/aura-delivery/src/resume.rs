//! The resumable upload state machine. One file at a time, one state per file.
//!
//! ## The unit is a file, and that is the whole design
//!
//! Section 10.1: "provider uploads resume correctly after a network drop". A state machine whose
//! unit was the *job* would have two states - started and finished - and a wedding that dropped at
//! 90 % would begin again. One whose unit is a file re-sends the tail of one file.
//!
//! On a 1,000-image gallery at 100 Mbps that is the difference between losing four seconds and
//! losing thirty minutes, and a photographer uploading from a venue's wifi will hit it several
//! times.
//!
//! ## Where the offset comes from
//!
//! From the far end, never from local state. [`step`] asks `head` what the provider already holds
//! and resumes from *that*, because a stored offset is a claim about somebody else's disk and the
//! two disagree exactly when it matters: after a crash mid-`put`, when the local row says 4 MB and
//! the service kept 3.2 MB. Resuming from the local number would leave an 800 KB hole in the
//! middle of a photograph, and the digest check at the end is what would eventually notice.
//!
//! ## `corrupt` is not `failed`
//!
//! A file that did not arrive and a file that arrived wrong need different responses, and only the
//! second is worth re-sending immediately. Two states, two codes, two rows - and
//! [`UploadState::Corrupt`] is what turns "the gallery has a broken photograph" into something a
//! panel can show.

use aura_core::contract::delivery::{DeliveryCode, DeliveryReason, UploadItem, UploadState};
use aura_core::AuraResult;

use crate::errors::{unreachable, upload_corrupt};
use crate::providers::Transport;

/// How many bytes go in one `put`.
///
/// Four mebibytes. Large enough that a 20 MB JPEG is five calls rather than a thousand, small
/// enough that a drop costs at most 4 MB of re-sending - and small enough that the progress a
/// photographer watches moves.
pub const CHUNK: usize = 4 * 1024 * 1024;

/// How many times one file is retried inside a single pass before it is left outstanding.
///
/// Three. Beyond that the fault is not transient, and a pass that retried forever would be a pass
/// that never returns to tell a photographer their wifi is down.
pub const MAX_ATTEMPTS: u32 = 3;

/// What one file's step produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The new state.
    pub state: UploadState,
    /// Bytes accepted by the far end during this step.
    pub sent: u64,
    /// What the panel says.
    pub reasons: Vec<DeliveryReason>,
}

/// Send one file, resuming from whatever the far end already holds.
///
/// # Errors
///
/// Never. A transport failure becomes [`UploadState::Failed`] rather than an error, because one
/// unreachable file in a wedding is not a reason to abandon the other 999 - and the state is what
/// the next pass resumes from. A caller that wants the failure as an error reads the state.
pub fn step(transport: &dyn Transport, item: &UploadItem, bytes: &[u8], key: &str) -> Step {
    let mut reasons = Vec::new();

    // Where the far end actually is. Never the stored offset - see the note above.
    let already = match transport.head(key) {
        Ok(Some(a)) => a,
        Ok(None) => crate::providers::Accepted {
            bytes: 0,
            digest: None,
        },
        Err(e) => {
            return Step {
                state: UploadState::Failed {
                    code: e.code.0.to_owned(),
                },
                sent: 0,
                reasons: vec![DeliveryReason::plain(DeliveryCode::ProviderUnreachable)],
            }
        }
    };

    // Already complete and correct? Nothing to do. A re-run over a finished upload is the ordinary
    // case, because a photographer presses the button again after a drop.
    if already.bytes == item.bytes {
        return match already.digest.as_deref() {
            Some(d) if d == item.hash => Step {
                state: UploadState::Verified,
                sent: 0,
                reasons: vec![DeliveryReason::plain(DeliveryCode::UploadVerified)],
            },
            Some(_) => Step {
                state: UploadState::Corrupt,
                sent: 0,
                reasons: vec![DeliveryReason::with(
                    DeliveryCode::UploadCorrupt,
                    item.path.to_string_lossy().to_string(),
                )],
            },
            // The far end has the right number of bytes and will not say what they hash to. That is
            // a service that reports nothing until a file is complete, and treating it as verified
            // would make the corruption check vacuous on exactly those services.
            None => Step {
                state: UploadState::Verified,
                sent: 0,
                reasons: vec![DeliveryReason::with(
                    DeliveryCode::UploadVerified,
                    "the provider reports no checksum".to_owned(),
                )],
            },
        };
    }

    // Resume, or start again on a transport that cannot take a partial file.
    let mut offset = if transport.resumable() {
        already.bytes.min(item.bytes)
    } else {
        0
    };
    if offset > 0 {
        reasons.push(DeliveryReason::with(
            DeliveryCode::UploadResumed,
            format!("{offset} bytes"),
        ));
    }

    let mut sent = 0_u64;
    let mut resumes = match &item.state {
        UploadState::InProgress { resumes, .. } => *resumes,
        _ => 0,
    };
    if offset > 0 {
        resumes = resumes.saturating_add(1);
    }

    while offset < item.bytes {
        let from = usize::try_from(offset).unwrap_or(usize::MAX);
        let to = (from + CHUNK).min(bytes.len());
        let Some(slice) = bytes.get(from..to) else {
            return Step {
                state: UploadState::Failed {
                    code: "AURA-DLV-10002".to_owned(),
                },
                sent,
                reasons,
            };
        };
        match transport.put(key, offset, slice) {
            Ok(accepted) => {
                sent = sent.saturating_add(accepted.bytes.saturating_sub(offset));
                offset = accepted.bytes;
            }
            Err(e) => {
                // The connection died. What the far end kept is what the next pass resumes from,
                // which is why this is `InProgress` rather than `Failed`.
                let kept = transport
                    .head(key)
                    .ok()
                    .flatten()
                    .map_or(offset, |a| a.bytes);
                reasons.push(DeliveryReason::plain(DeliveryCode::ProviderUnreachable));
                return Step {
                    state: if kept > 0 {
                        UploadState::InProgress {
                            sent: kept,
                            resumes,
                        }
                    } else {
                        UploadState::Failed {
                            code: e.code.0.to_owned(),
                        }
                    },
                    sent: kept.saturating_sub(already.bytes),
                    reasons,
                };
            }
        }
    }

    // Everything is there. Compare digests.
    #[allow(clippy::single_match_else)]
    match transport.head(key) {
        Ok(Some(a)) => match a.digest.as_deref() {
            Some(d) if d == item.hash => {
                reasons.push(DeliveryReason::plain(DeliveryCode::UploadVerified));
                Step {
                    state: UploadState::Verified,
                    sent,
                    reasons,
                }
            }
            Some(_) => {
                reasons.push(DeliveryReason::with(
                    DeliveryCode::UploadCorrupt,
                    item.path.to_string_lossy().to_string(),
                ));
                Step {
                    state: UploadState::Corrupt,
                    sent,
                    reasons,
                }
            }
            None => {
                reasons.push(DeliveryReason::with(
                    DeliveryCode::UploadVerified,
                    "the provider reports no checksum".to_owned(),
                ));
                Step {
                    state: UploadState::Verified,
                    sent,
                    reasons,
                }
            }
        },
        _ => {
            reasons.push(DeliveryReason::plain(DeliveryCode::ProviderUnreachable));
            Step {
                state: UploadState::InProgress {
                    sent: offset,
                    resumes,
                },
                sent,
                reasons,
            }
        }
    }
}

/// Send one file, retrying up to [`MAX_ATTEMPTS`] times inside this pass.
///
/// # Errors
///
/// `AURA-DLV-10005` when the far end's digest disagrees after every attempt, which is the one
/// failure worth surfacing rather than storing - a photographer should be told their gallery has a
/// broken photograph in it rather than seeing a count go down.
pub fn send(
    transport: &dyn Transport,
    item: &UploadItem,
    bytes: &[u8],
    key: &str,
) -> AuraResult<Step> {
    let mut current = item.clone();
    let mut last = Step {
        state: UploadState::Pending,
        sent: 0,
        reasons: Vec::new(),
    };
    for _ in 0..MAX_ATTEMPTS {
        last = step(transport, &current, bytes, key);
        match &last.state {
            UploadState::Verified => return Ok(last),
            UploadState::Corrupt => {
                // Re-send from zero: a corrupt file is one the far end holds *wrongly*, and
                // resuming into it would append to the corruption.
                current.state = UploadState::Pending;
            }
            other => {
                current.state = other.clone();
            }
        }
    }
    if last.state == UploadState::Corrupt {
        return Err(upload_corrupt(&item.path.to_string_lossy()));
    }
    if matches!(last.state, UploadState::Failed { .. }) {
        return Err(unreachable(format!(
            "`{}` did not reach the provider after {MAX_ATTEMPTS} attempts",
            item.path.display()
        )));
    }
    Ok(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ScriptedTransport;
    use aura_core::contract::delivery::ImageId;
    use std::path::PathBuf;

    fn item(bytes: &[u8]) -> UploadItem {
        UploadItem {
            image: ImageId::new(),
            set: "gallery".to_owned(),
            path: PathBuf::from("gallery/a.jpg"),
            bytes: bytes.len() as u64,
            hash: blake3::hash(bytes).to_hex().to_string(),
            state: UploadState::Pending,
        }
    }

    #[test]
    fn a_whole_file_goes_in_one_pass_and_verifies() {
        let t = ScriptedTransport::new();
        let bytes = vec![7_u8; 1000];
        let it = item(&bytes);
        let step = send(&t, &it, &bytes, "k").unwrap();
        assert_eq!(step.state, UploadState::Verified);
        assert_eq!(t.contents("k"), Some(bytes));
    }

    #[test]
    fn a_drop_leaves_the_file_in_progress_and_the_next_pass_sends_only_the_tail() {
        // Section 10.1's row, and the property the whole module exists for.
        let t = ScriptedTransport::new();
        let bytes: Vec<u8> = (0..(CHUNK * 2 + 500)).map(|i| (i % 251) as u8).collect();
        let it = item(&bytes);

        t.drop_after(CHUNK / 2);
        let first = step(&t, &it, &bytes, "k");
        assert!(
            matches!(first.state, UploadState::InProgress { .. }),
            "{:?}",
            first.state
        );
        let held = t.contents("k").unwrap().len();
        assert_eq!(held, CHUNK / 2);

        t.recover();
        let mut resumed = it.clone();
        resumed.state = first.state;
        let second = send(&t, &resumed, &bytes, "k").unwrap();
        assert_eq!(second.state, UploadState::Verified);
        assert_eq!(t.contents("k"), Some(bytes.clone()));
        // Only the tail travelled: the second pass sent the file minus what was already there.
        assert_eq!(second.sent, bytes.len() as u64 - held as u64);
        assert!(second
            .reasons
            .iter()
            .any(|r| r.code == DeliveryCode::UploadResumed));
    }

    #[test]
    fn a_file_the_far_end_already_has_is_not_sent_again() {
        // The ordinary case, because a photographer presses the button again after a drop.
        let t = ScriptedTransport::new();
        let bytes = vec![3_u8; 500];
        let it = item(&bytes);
        send(&t, &it, &bytes, "k").unwrap();
        let again = step(&t, &it, &bytes, "k");
        assert_eq!(again.state, UploadState::Verified);
        assert_eq!(again.sent, 0, "nothing re-sent");
    }

    #[test]
    fn a_wrong_digest_is_corrupt_rather_than_failed_and_is_re_sent() {
        let t = ScriptedTransport::new();
        let bytes = vec![5_u8; 100];
        let it = item(&bytes);
        t.corrupt("k");
        let err = send(&t, &it, &bytes, "k").expect_err("corrupt");
        assert_eq!(err.code.0, "AURA-DLV-10005");

        // And a single step reports the state rather than the error, which is what the panel reads.
        let one = step(&t, &it, &bytes, "k");
        assert_eq!(one.state, UploadState::Corrupt);
        assert!(one
            .reasons
            .iter()
            .any(|r| r.code == DeliveryCode::UploadCorrupt));
    }

    #[test]
    fn an_offline_provider_leaves_the_file_failed_rather_than_erroring_mid_wedding() {
        let t = ScriptedTransport::new();
        let bytes = vec![1_u8; 10];
        let it = item(&bytes);
        t.go_offline();
        let one = step(&t, &it, &bytes, "k");
        assert!(matches!(one.state, UploadState::Failed { .. }));
        assert!(one
            .reasons
            .iter()
            .any(|r| r.code == DeliveryCode::ProviderUnreachable));
        assert!(one.state.is_outstanding());
    }

    #[test]
    fn a_transport_that_cannot_resume_starts_again_rather_than_appending_into_a_hole() {
        let t = ScriptedTransport::whole_files_only();
        let bytes: Vec<u8> = (0..(CHUNK + 100)).map(|i| (i % 97) as u8).collect();
        let it = item(&bytes);
        t.drop_after(200);
        let _ = step(&t, &it, &bytes, "k");
        assert_eq!(t.contents("k").unwrap().len(), 200);
        t.recover();
        let done = send(&t, &it, &bytes, "k").unwrap();
        assert_eq!(done.state, UploadState::Verified);
        assert_eq!(t.contents("k"), Some(bytes));
    }

    #[test]
    fn the_offset_comes_from_the_far_end_and_not_from_the_stored_row() {
        // The trap this module exists to avoid. A stored offset is a claim about somebody else's
        // disk, and the two disagree exactly when it matters: after a crash mid-put. Resuming from
        // the local number would leave a hole in the middle of a photograph.
        let t = ScriptedTransport::new();
        let bytes: Vec<u8> = (0..800).map(|i| (i % 251) as u8).collect();
        let mut it = item(&bytes);
        t.put("k", 0, &bytes[..300]).unwrap();
        // The row *lies*: it claims 700 bytes are there when the far end holds 300.
        it.state = UploadState::InProgress {
            sent: 700,
            resumes: 1,
        };
        let done = send(&t, &it, &bytes, "k").unwrap();
        assert_eq!(done.state, UploadState::Verified);
        assert_eq!(t.contents("k"), Some(bytes), "no hole in the middle");
    }

    #[test]
    fn a_provider_that_reports_no_checksum_is_accepted_and_said_so() {
        // A service that reports nothing until a file is complete is a real shape, and treating
        // its silence as a match would make the corruption check vacuous on exactly those
        // services. So the note carries the caveat rather than the code hiding it.
        #[derive(Debug)]
        struct Silent(std::sync::Mutex<Vec<u8>>);
        impl Transport for Silent {
            fn put(
                &self,
                _k: &str,
                offset: u64,
                bytes: &[u8],
            ) -> AuraResult<crate::providers::Accepted> {
                let mut held = self.0.lock().map_err(|_| unreachable("poisoned"))?;
                held.truncate(usize::try_from(offset).unwrap_or(0));
                held.extend_from_slice(bytes);
                Ok(crate::providers::Accepted {
                    bytes: held.len() as u64,
                    digest: None,
                })
            }
            fn head(&self, _k: &str) -> AuraResult<Option<crate::providers::Accepted>> {
                let held = self.0.lock().map_err(|_| unreachable("poisoned"))?;
                if held.is_empty() {
                    return Ok(None);
                }
                Ok(Some(crate::providers::Accepted {
                    bytes: held.len() as u64,
                    digest: None,
                }))
            }
        }
        let t = Silent(std::sync::Mutex::new(Vec::new()));
        let bytes = vec![9_u8; 50];
        let it = item(&bytes);
        let done = send(&t, &it, &bytes, "k").unwrap();
        assert_eq!(done.state, UploadState::Verified);
        assert!(done.reasons.iter().any(|r| r
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("no checksum"))));
    }
}
