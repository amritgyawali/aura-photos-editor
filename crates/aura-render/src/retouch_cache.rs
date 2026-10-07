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
const BUDGET: usize = 1024 * 1024 * 1024;
const ENTRIES: usize = 16;

#[derive(Debug, Default)]
struct Store {
    entries: VecDeque<(String, Arc<Checkpoint>)>,
    bytes: usize,
}

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
    }

    pub(crate) fn get(&self, key: &str) -> Option<Arc<Checkpoint>> {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        let index = store.entries.iter().position(|(saved, _)| saved == key)?;
        let entry = store.entries.remove(index)?;
        let found = Arc::clone(&entry.1);
        store.entries.push_back(entry);
        Some(found)
    }

    pub(crate) fn put(&self, key: String, checkpoint: Checkpoint) {
        let bytes = checkpoint.bytes();
        if bytes > BUDGET {
            return;
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
        store.entries.push_back((key, Arc::new(checkpoint)));
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
