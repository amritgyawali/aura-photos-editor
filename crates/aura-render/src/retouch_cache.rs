//! Retouch checkpoints: the working buffer just before the retouch stack, and just after it,
//! kept so the next render of the same photograph does not repeat work it already did.
//! ADR-0098.
//!
//! Retouching is where nearly all of a portrait's render time goes, and the way it is edited
//! has a shape: an operation is added at the end of the stack, a draft at the end is replaced
//! while a brush moves, a before view leaves the stack off. In each case everything up to some
//! point is exactly what the last render computed. A checkpoint is that buffer, keyed by
//! everything that produced it, so a render can start from it:
//!
//! - **before the stack** - every stage up to retouching, keyed by the photograph, the level,
//!   the purpose and the recipe with the stack and the stages after it left out;
//! - **after the stack** - keyed by that, the mattes and every operation in order (its id left
//!   out, which does not change a pixel), so the stack plus one more operation runs one.
//!
//! The result is identical to rendering from scratch: an operation resumed from a checkpoint
//! reads its mattes and texture references from the before-the-stack buffer, which is what it
//! reads in a full render. Where that cannot be made true - a texture restore that measures the
//! skin acne clear left, with the acne clear already inside the checkpoint - the render starts
//! from the before-the-stack checkpoint instead.
use rayon::prelude::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use crate::contract::render::RenderNote;
use crate::spatial;

/// A working buffer and what the stages before it reported.
#[derive(Debug)]
pub(crate) struct Checkpoint {
    pub rgb: Vec<f32>,
    pub width: u32,
    pub height: u32,
    pub notes: Vec<RenderNote>,
    pub stats: spatial::Stats,
}

impl Checkpoint {
    fn bytes(&self) -> usize {
        self.rgb.len().saturating_mul(4)
    }
}

/// Memory kept for checkpoints: a few full-resolution frames, or many screen-sized ones.
const BUDGET: usize = 256 * 1024 * 1024;
/// Live-preview pairs retain their own strong references even after entry eviction.
/// Bound them independently so evicting a checkpoint actually releases memory.
const LATEST_BUDGET: usize = 128 * 1024 * 1024;
const ENTRIES: usize = 16;

#[derive(Debug, Default)]
struct Store {
    entries: VecDeque<(String, Arc<Checkpoint>)>,
    bytes: usize,
    /// The newest before-and-after pair per photograph and size, for a live preview.
    latest: VecDeque<Latest>,
}

/// The buffers either side of one photograph's retouch stack, from its most recent render.
#[derive(Debug, Clone)]
pub(crate) struct Latest {
    /// The photograph, the size and the purpose.
    pub scope: String,
    /// The stack and its mattes, independent of the settings before it.
    pub stack: String,
    pub before: Arc<Checkpoint>,
    pub after: Arc<Checkpoint>,
}

/// How many stacks keep a pair for a live preview: a photograph's saved stack and the draft on
/// top of it each have their own, for a few photographs.
const LATEST: usize = 6;

/// Checkpoints shared by every engine built over one catalog. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct Checkpoints {
    store: Arc<Mutex<Store>>,
}

impl Checkpoints {
    /// An empty set of checkpoints.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget every checkpoint.
    pub fn clear(&self) {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        store.entries.clear();
        store.bytes = 0;
        store.latest.clear();
    }

    /// Remember the newest pair either side of a photograph's stack.
    pub(crate) fn remember(&self, latest: Latest) {
        self.remember_with_budget(latest, LATEST_BUDGET);
    }

    fn remember_with_budget(&self, latest: Latest, budget: usize) {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        store
            .latest
            .retain(|saved| saved.scope != latest.scope || saved.stack != latest.stack);
        let size = latest.before.bytes().saturating_add(latest.after.bytes());
        if size > budget {
            return;
        }
        while store.latest.len() >= LATEST
            || store.latest.iter().fold(size, |bytes, saved| {
                bytes
                    .saturating_add(saved.before.bytes())
                    .saturating_add(saved.after.bytes())
            }) > budget
        {
            store.latest.pop_front();
        }
        store.latest.push_back(latest);
    }

    /// The newest pair for `scope` and `stack`, when there is one.
    pub(crate) fn latest(&self, scope: &str, stack: &str) -> Option<Latest> {
        let store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        store
            .latest
            .iter()
            .rev()
            .find(|saved| saved.scope == scope && saved.stack == stack)
            .cloned()
    }

    pub(crate) fn get(&self, key: &str) -> Option<Arc<Checkpoint>> {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        let index = store.entries.iter().position(|(saved, _)| saved == key)?;
        let entry = store.entries.remove(index)?;
        let found = Arc::clone(&entry.1);
        store.entries.push_back(entry);
        Some(found)
    }

    pub(crate) fn put(&self, key: String, checkpoint: Checkpoint) -> Arc<Checkpoint> {
        let bytes = checkpoint.bytes();
        let checkpoint = Arc::new(checkpoint);
        if bytes > BUDGET {
            return checkpoint;
        }
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = store.entries.iter().position(|(saved, _)| *saved == key) {
            if let Some((_, previous)) = store.entries.remove(index) {
                store.bytes = store.bytes.saturating_sub(previous.bytes());
            }
        }
        while store.bytes.saturating_add(bytes) > BUDGET || store.entries.len() >= ENTRIES {
            let Some((_, previous)) = store.entries.pop_front() else {
                break;
            };
            store.bytes = store.bytes.saturating_sub(previous.bytes());
        }
        store.bytes += bytes;
        store.entries.push_back((key, Arc::clone(&checkpoint)));
        checkpoint
    }
}

/// The key after each operation of the stack, starting from the key of the buffer before it.
/// `keys[k]` names the buffer after the first `k` operations.
pub(crate) fn prefix_keys(
    before: &str,
    edits: &[aura_recipe::retouch_tools::Edit],
    mattes: &std::collections::BTreeMap<String, aura_recipe::retouch_tools::Matte>,
) -> Vec<String> {
    let mut keys = Vec::with_capacity(edits.len() + 1);
    let mut hasher = blake3::Hasher::new();
    hasher.update(before.as_bytes());
    hasher.update(serde_json::to_string(mattes).unwrap_or_default().as_bytes());
    keys.push(hasher.finalize().to_hex().to_string());
    for edit in edits {
        let mut op = edit.clone();
        // The id names the operation; it changes no pixel.
        op.id.clear();
        let mut next = blake3::Hasher::new();
        next.update(keys.last().map_or("", String::as_str).as_bytes());
        next.update(serde_json::to_string(&op).unwrap_or_default().as_bytes());
        keys.push(next.finalize().to_hex().to_string());
    }
    keys
}

/// A key for the stack and its mattes alone - the same whatever the settings before it.
pub(crate) fn stack_key(
    edits: &[aura_recipe::retouch_tools::Edit],
    mattes: &std::collections::BTreeMap<String, aura_recipe::retouch_tools::Matte>,
) -> String {
    prefix_keys("", edits, mattes).pop().unwrap_or_default()
}

/// The live preview's estimate of the stack's effect on a new frame: each channel of the frame
/// before the stack is scaled by how much the stack scaled it last time. Exact for any change
/// that scales the frame before the stack (exposure, white balance); close for the rest. It is
/// only ever shown until the exact render arrives. ADR-0099.
pub(crate) fn carry_over(before: &mut [f32], last: &Latest) {
    const FLOOR: f32 = 1e-3;
    if last.before.rgb.len() != before.len() || last.after.rgb.len() != before.len() {
        return;
    }
    before
        .par_iter_mut()
        .zip(last.before.rgb.par_iter())
        .zip(last.after.rgb.par_iter())
        .for_each(|((value, was), became)| {
            let ratio = ((became.max(0.0) + FLOOR) / (was.max(0.0) + FLOOR)).clamp(0.0, 8.0);
            *value = (value.max(0.0) + FLOOR) * ratio - FLOOR;
        });
}

/// Whether operations from `start` on may resume from a checkpoint taken after the first
/// `start`: not when a texture restore that measures the skin acne clear left comes after an
/// acne clear the checkpoint already contains, since that measurement is not kept.
pub(crate) fn may_resume(edits: &[aura_recipe::retouch_tools::Edit], start: usize) -> bool {
    use aura_recipe::retouch_tools::Tool;
    let cleared_before = edits
        .iter()
        .take(start)
        .any(|e| e.enabled && e.amount > 0.0 && e.tool == Tool::AcneClear);
    let measures_after = edits.iter().skip(start).any(|e| {
        e.enabled && e.amount > 0.0 && e.tool == Tool::TextureGraft && e.preserve_microtexture
    });
    !(cleared_before && measures_after)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn checkpoint(len: usize) -> Checkpoint {
        Checkpoint {
            rgb: vec![0.5; len],
            width: 1,
            height: 1,
            notes: Vec::new(),
            stats: spatial::Stats::default(),
        }
    }

    #[test]
    fn the_store_is_bounded_and_keeps_the_most_recently_used() {
        let cache = Checkpoints::new();
        for n in 0..ENTRIES {
            cache.put(n.to_string(), checkpoint(3));
        }
        assert!(cache.get("0").is_some());
        cache.put("new".into(), checkpoint(3));
        assert!(cache.get("1").is_none());
        assert!(cache.get("0").is_some());
        cache.clear();
        assert!(cache.get("0").is_none());
    }

    #[test]
    fn live_pairs_release_evicted_buffers_and_reject_oversized_pairs() {
        let cache = Checkpoints::new();
        let pair = |scope: &str, pixels: usize| Latest {
            scope: scope.into(),
            stack: "saved".into(),
            before: Arc::new(checkpoint(pixels)),
            after: Arc::new(checkpoint(pixels)),
        };
        let first = pair("first", 3);
        let old_buffer = Arc::downgrade(&first.before);
        cache.remember_with_budget(first, 48);
        cache.remember_with_budget(pair("second", 3), 48);
        assert!(old_buffer.upgrade().is_some());
        cache.remember_with_budget(pair("third", 3), 48);
        assert!(
            old_buffer.upgrade().is_none(),
            "eviction still pins a buffer"
        );
        assert!(cache.latest("first", "saved").is_none());
        assert!(cache.latest("second", "saved").is_some());
        assert!(cache.latest("third", "saved").is_some());
        cache.remember_with_budget(pair("oversized", 9), 48);
        assert!(cache.latest("oversized", "saved").is_none());
        cache.clear();
        assert!(cache.latest("third", "saved").is_none());
    }

    #[test]
    fn a_renamed_operation_keeps_its_key_and_a_changed_one_does_not() {
        let edit = aura_recipe::retouch_tools::Edit {
            id: "a".into(),
            tool: aura_recipe::retouch_tools::Tool::Heal,
            enabled: true,
            region: [0.5, 0.5, 0.1, 0.1],
            source: None,
            amount: 0.5,
            feather: 0.5,
            radius: 0.01,
            source_scale: 1.0,
            preserve_microtexture: false,
            texture_heal: false,
            clean_ring_fit: false,
            curved_heal: false,
            heal_samples: Vec::new(),
            texture_sources: Vec::new(),
            sensitivity: None,
            keep_dark_marks: false,
            texture: 1.0,
            tone: 0.5,
            warmth: 0.0,
            tint: 0.0,
            mask: None,
            skin: None,
            selection: None,
            matte: None,
        };
        let mattes = std::collections::BTreeMap::new();
        let mut renamed = edit.clone();
        renamed.id = "b".into();
        let mut stronger = edit.clone();
        stronger.amount = 0.6;
        let base = prefix_keys("k", std::slice::from_ref(&edit), &mattes);
        assert_eq!(base, prefix_keys("k", &[renamed], &mattes));
        assert_ne!(base[1], prefix_keys("k", &[stronger], &mattes)[1]);
        assert_eq!(base[0], prefix_keys("k", &[], &mattes)[0]);
    }
}
