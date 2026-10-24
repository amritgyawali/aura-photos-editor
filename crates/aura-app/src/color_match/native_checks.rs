//! Bounded process-only proofs. A cache key includes every actual rendered pixel and guard
//! context. Changed pixels, measurements or region points cannot reuse an earlier result.
//! Proofs are never deserialized from catalogue settings or portable reference files.
use aura_raw::codec::Rgb8;
use parking_lot::Mutex;
use std::{collections::VecDeque, sync::Arc};

use super::{appearance::Region, invalid, transfer, AuraResult};

const CAPACITY: usize = 64;
pub(super) type Digest = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    pub before: Digest,
    pub after: Digest,
    pub context: Digest,
}

#[derive(Debug)]
pub(super) struct Guard {
    pub regions: Vec<Region>,
    pub clipped: f32,
}

#[derive(Debug, Clone)]
struct Proof {
    key: Key,
    before: transfer::Distance,
    after: transfer::Distance,
    guard: Arc<Guard>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NativeChecks {
    proofs: Arc<Mutex<VecDeque<Proof>>>,
}

impl NativeChecks {
    pub(super) fn baseline(
        &self,
        before: Digest,
        context: Digest,
    ) -> Option<(transfer::Distance, Arc<Guard>)> {
        self.proofs
            .lock()
            .iter()
            .rev()
            .find(|proof| proof.key.before == before && proof.key.context == context)
            .map(|proof| (proof.before, Arc::clone(&proof.guard)))
    }

    pub(super) fn checked(&self, key: Key) -> Option<transfer::Distance> {
        self.proofs
            .lock()
            .iter()
            .rev()
            .find(|proof| proof.key == key)
            .map(|proof| proof.after)
    }

    pub(super) fn remember(
        &self,
        key: Key,
        before: transfer::Distance,
        after: transfer::Distance,
        guard: Arc<Guard>,
    ) {
        if !before.total.is_finite()
            || !after.total.is_finite()
            || after.total > before.total + 0.0001
        {
            return;
        }
        let mut proofs = self.proofs.lock();
        proofs.retain(|proof| proof.key != key);
        if proofs.len() >= CAPACITY {
            let _ = proofs.pop_front();
        }
        proofs.push_back(Proof {
            key,
            before,
            after,
            guard,
        });
    }
}

pub(super) fn image_digest(image: &Rgb8) -> Digest {
    let mut hash = blake3::Hasher::new();
    hash.update(&image.width.to_le_bytes());
    hash.update(&image.height.to_le_bytes());
    hash.update(&image.data);
    *hash.finalize().as_bytes()
}

pub(super) fn context_digest(
    reference: &transfer::GradeStats,
    regions: &[Region],
    monochrome: bool,
) -> AuraResult<Digest> {
    // JSON encodes non-finite floats as null. Reject malformed stored measurements or
    // coordinates so distinct invalid inputs can never alias a valid verification context.
    if !reference.usable()
        || regions
            .iter()
            .flat_map(|region| &region.points)
            .flatten()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(invalid("The native colour-check measurements are invalid. Analyse the reference again. No edit was saved."));
    }
    let points: Vec<_> = regions
        .iter()
        .map(|region| (&region.name, &region.points))
        .collect();
    let bytes = serde_json::to_vec(&(reference, points, monochrome))
        .map_err(|_| invalid("The native colour-check context could not be encoded."))?;
    Ok(*blake3::hash(&bytes).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::look::LookAggregate;

    fn distance(total: f32) -> transfer::Distance {
        transfer::Distance {
            tone: total,
            cast: total,
            chroma: total,
            hue: total,
            palette: total,
            total,
        }
    }

    #[test]
    fn measurement_context_is_stable_across_session_storage_and_changes_with_its_inputs() {
        let reference = transfer::measure(&[[0.5, 0.02, -0.01]; 256]);
        let mut regions = vec![Region {
            name: "guard:person-1 skin".into(),
            points: vec![[0.5, 0.5]],
            appearance: LookAggregate::default(),
        }];
        let original = context_digest(&reference, &regions, false).unwrap();
        let saved = serde_json::to_string(&(&reference, &regions)).unwrap();
        let restored: (transfer::GradeStats, Vec<Region>) = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            original,
            context_digest(&restored.0, &restored.1, false).unwrap()
        );
        assert_ne!(
            original,
            context_digest(&reference, &regions, true).unwrap()
        );
        let mut different = reference.clone();
        different
            .lightness
            .iter_mut()
            .for_each(|value| *value += 0.01);
        assert_ne!(
            original,
            context_digest(&different, &regions, false).unwrap()
        );
        if let Some(region) = regions.first_mut() {
            region.points.push([0.6, 0.6]);
        }
        assert_ne!(
            original,
            context_digest(&reference, &regions, false).unwrap()
        );
        if let Some(region) = regions.first_mut() {
            region.points.push([f32::INFINITY, 0.5]);
        }
        assert!(context_digest(&reference, &regions, false).is_err());
        different
            .lightness
            .iter_mut()
            .for_each(|value| *value = f32::NAN);
        assert!(context_digest(&different, &[], false).is_err());
    }

    #[test]
    fn changed_pixels_dimensions_or_guard_context_invalidate_native_proofs() {
        let checks = NativeChecks::default();
        let mut image = Rgb8 {
            width: 2,
            height: 1,
            data: vec![60, 70, 80, 90, 100, 110],
        };
        let digest = image_digest(&image);
        let key = Key {
            before: digest,
            after: digest,
            context: [1; 32],
        };
        checks.remember(
            key,
            distance(4.0),
            distance(2.0),
            Arc::new(Guard {
                regions: Vec::new(),
                clipped: 0.0,
            }),
        );
        assert_eq!(
            checks.checked(key).map(|v| v.total.to_bits()),
            Some(2.0_f32.to_bits())
        );
        if let Some(value) = image.data.first_mut() {
            *value += 1;
        }
        assert!(checks
            .checked(Key {
                after: image_digest(&image),
                ..key
            })
            .is_none());
        assert!(checks
            .checked(Key {
                before: image_digest(&image),
                ..key
            })
            .is_none());
        assert!(checks
            .checked(Key {
                context: [2; 32],
                ..key
            })
            .is_none());
        if let Some(value) = image.data.first_mut() {
            *value -= 1;
        }
        image.width = 1;
        image.height = 2;
        assert_ne!(image_digest(&image), digest);
        assert!(NativeChecks::default().checked(key).is_none());
    }

    #[test]
    fn only_a_bounded_set_of_nonregressing_in_process_results_is_retained() {
        let checks = NativeChecks::default();
        let key = Key {
            before: [0; 32],
            after: [0; 32],
            context: [0; 32],
        };
        let guard = Arc::new(Guard {
            regions: Vec::new(),
            clipped: 0.0,
        });
        checks.remember(key, distance(1.0), distance(2.0), Arc::clone(&guard));
        assert!(checks.checked(key).is_none());
        for index in 0..=CAPACITY {
            let mut next = key;
            next.after = *blake3::hash(&index.to_le_bytes()).as_bytes();
            checks.remember(next, distance(4.0), distance(2.0), Arc::clone(&guard));
        }
        assert_eq!(checks.proofs.lock().len(), CAPACITY);
        let oldest = Key {
            after: *blake3::hash(&0_usize.to_le_bytes()).as_bytes(),
            ..key
        };
        assert!(checks.checked(oldest).is_none());
        assert!(checks.baseline(key.before, key.context).is_some());
    }
}
