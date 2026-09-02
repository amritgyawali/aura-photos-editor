//! Per-set mapping: which set lands in which of a provider's collections.
//!
//! ## Getting this wrong publishes a whole wedding on the wedding night
//!
//! Section 6.2 asks for per-set mapping, and the reason it is a first-class shape rather than a
//! string concatenation is what happens when it is wrong. A gallery goes to the client's main
//! collection, a teaser to a preview one, an album to a private one. A default that sent everything
//! to the first collection would put the album, the outtakes and the full gallery in front of a
//! couple who were expecting five photographs.
//!
//! ## An unmapped set is left out, loudly
//!
//! Not sent to a default collection, and not a hard failure either. A photographer who mapped the
//! gallery and not the album usually meant to, so [`DeliveryCode::SetUnmapped`] is a warning that
//! names the set - and the upload of everything that *is* mapped goes ahead.
//!
//! Phase 24's rule, at its cheapest: an absent mapping is ignorance, not permission, and "we sent it
//! somewhere sensible" is the one response that cannot be taken back.

use std::collections::BTreeMap;

use aura_core::contract::delivery::{DeliveryCode, DeliveryReason, SetMapping};

/// A validated mapping from set names to remote collections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mapping {
    rows: BTreeMap<String, SetMapping>,
}

impl Mapping {
    /// Build a mapping, dropping duplicates by taking the last.
    #[must_use]
    pub fn new(rows: &[SetMapping]) -> Self {
        let mut map = BTreeMap::new();
        for row in rows {
            map.insert(row.set.clone(), row.clone());
        }
        Self { rows: map }
    }

    /// Where a set goes, or `None`.
    #[must_use]
    pub fn get(&self, set: &str) -> Option<&SetMapping> {
        self.rows.get(set)
    }

    /// Every row, in set order.
    #[must_use]
    pub fn rows(&self) -> Vec<&SetMapping> {
        self.rows.values().collect()
    }

    /// How many sets are mapped.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether nothing is mapped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Split a delivery's set names into the mapped and the unmapped, with a reason for each of the
    /// second.
    ///
    /// Both halves, rather than filtering: `DeliveryOutline::unmapped_sets` is a number a
    /// photographer acts on, and a filter that returned only the mapped half would leave the panel
    /// with nothing to say about why an album never arrived.
    #[must_use]
    pub fn split<'a>(
        &self,
        sets: &'a [String],
    ) -> (Vec<&'a String>, Vec<(&'a String, DeliveryReason)>) {
        let mut mapped = Vec::new();
        let mut unmapped = Vec::new();
        for set in sets {
            if self.rows.contains_key(set) {
                mapped.push(set);
            } else {
                unmapped.push((
                    set,
                    DeliveryReason::with(DeliveryCode::SetUnmapped, set.clone()),
                ));
            }
        }
        (mapped, unmapped)
    }

    /// Whether any row asks to publish on upload.
    ///
    /// Read by the upload before it starts, because a provider whose `may_publish` is false must
    /// refuse the whole mapping rather than silently ignoring the flag - a photographer who ticked
    /// "publish" and was quietly not published is a photographer who thinks their client has the
    /// gallery.
    #[must_use]
    pub fn wants_publish(&self) -> bool {
        self.rows.values().any(|r| r.publish)
    }

    /// The same mapping with every publish flag cleared, and a note per row that was cleared.
    #[must_use]
    pub fn without_publish(&self) -> (Self, Vec<DeliveryReason>) {
        let mut notes = Vec::new();
        let mut rows = BTreeMap::new();
        for (name, row) in &self.rows {
            if row.publish {
                notes.push(DeliveryReason::with(
                    DeliveryCode::LeftUnpublished,
                    name.clone(),
                ));
            }
            rows.insert(
                name.clone(),
                SetMapping {
                    publish: false,
                    ..row.clone()
                },
            );
        }
        (Self { rows }, notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(set: &str, remote: &str, publish: bool) -> SetMapping {
        SetMapping {
            set: set.to_owned(),
            remote: remote.to_owned(),
            publish,
        }
    }

    #[test]
    fn an_unmapped_set_is_named_rather_than_sent_somewhere_sensible() {
        let mapping = Mapping::new(&[m("gallery", "main", false)]);
        let sets = vec!["gallery".to_owned(), "album".to_owned()];
        let (mapped, unmapped) = mapping.split(&sets);
        assert_eq!(mapped, vec![&"gallery".to_owned()]);
        assert_eq!(unmapped.len(), 1);
        assert_eq!(unmapped[0].1.code, DeliveryCode::SetUnmapped);
        assert_eq!(unmapped[0].1.detail.as_deref(), Some("album"));
    }

    #[test]
    fn clearing_publish_leaves_a_note_per_row_it_cleared() {
        // A photographer who ticked "publish" and was quietly not published is a photographer who
        // thinks their client has the gallery.
        let mapping = Mapping::new(&[m("gallery", "main", true), m("teaser", "preview", false)]);
        assert!(mapping.wants_publish());
        let (cleared, notes) = mapping.without_publish();
        assert!(!cleared.wants_publish());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].detail.as_deref(), Some("gallery"));
        assert_eq!(notes[0].code, DeliveryCode::LeftUnpublished);
    }

    #[test]
    fn a_later_row_replaces_an_earlier_one_for_the_same_set() {
        let mapping = Mapping::new(&[m("gallery", "old", false), m("gallery", "new", false)]);
        assert_eq!(mapping.len(), 1);
        assert_eq!(mapping.get("gallery").unwrap().remote, "new");
    }

    #[test]
    fn an_empty_mapping_leaves_every_set_out_and_says_so_for_each() {
        let mapping = Mapping::default();
        assert!(mapping.is_empty());
        let sets = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let (mapped, unmapped) = mapping.split(&sets);
        assert!(mapped.is_empty());
        assert_eq!(unmapped.len(), 3);
    }
}
