//! The ritual taxonomy: five shipped files, loaded as one table.
//!
//! Section 2.1 asks for "a ritual sub-classifier for tradition-specific events with an
//! extensible taxonomy file", and section 12's first failure mode is cultural blind
//! spots. Both of those are answered by the same decision: rituals live in config, and
//! `docs/adding-a-tradition.md` is a procedure a photographer's consultant can follow
//! without a compiler.
//!
//! ## The union, and why it is one table
//!
//! The five files are loaded into **one** [`Taxonomy`]. A Nepali wedding has a haldi
//! and so does a Hindu one; a civil ceremony borrows handfasting from `christian.toml`.
//! Declaring each rite in every file that might use it would guarantee that the copies
//! drift, so a rite is declared once, in the file where it originates, and every
//! tradition matches it through the union.
//!
//! What the `tradition` field on a rite then means is **where it was declared**, not
//! who may use it. [`Taxonomy::for_tradition`] returns the rites a tradition typically
//! has, which is a superset question and is used only to condition the head.
//!
//! ## The id is the model's output slot
//!
//! This is the load-bearing sentence of the whole module.
//! `aura_infer::onnx::fixtures::RITUAL_SLOTS` is 160 and the ritual head emits one
//! logit per slot; the slot **is** the `id` written in the TOML file. So:
//!
//! * an id must never be reused, because reusing one relabels a trained output;
//! * an id must never be derived from file order, because inserting a rite would
//!   renumber every rite after it;
//! * ids must be unique across every file, because two rites cannot share a logit.
//!
//! [`Taxonomy::load`] enforces all three and refuses with `AURA-ML-5024` rather than
//! loading a table that would quietly mean something else. Slot 0 is
//! [`RitualId::NONE`], the abstention, and is reserved.

use std::collections::BTreeMap;

use aura_core::contract::scene::RitualId;
use aura_core::AuraError;
use serde::Deserialize;

use crate::errors;

/// The five shipped taxonomies, embedded so a build can never disagree with itself.
///
/// Order matters only for the error message a duplicate id produces - the union is a
/// map - and it is the order the phase document lists the traditions in.
const EMBEDDED: &[(&str, &str)] = &[
    (
        "hindu.toml",
        include_str!("../../config/rituals/hindu.toml"),
    ),
    (
        "nepali.toml",
        include_str!("../../config/rituals/nepali.toml"),
    ),
    (
        "christian.toml",
        include_str!("../../config/rituals/christian.toml"),
    ),
    (
        "muslim.toml",
        include_str!("../../config/rituals/muslim.toml"),
    ),
    (
        "civil.toml",
        include_str!("../../config/rituals/civil.toml"),
    ),
];

/// The lowest id a taxonomy file may author. Zero is [`RitualId::NONE`].
pub const FIRST_ID: u16 = 1;

/// One past the highest id a taxonomy file may author.
///
/// Equal to the head's slot count, and that equality is the point. A test asserts it.
pub const SLOT_COUNT: u16 = aura_infer::onnx::fixtures::RITUAL_SLOTS as u16;

/// One rite, as authored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rite {
    /// The model output slot. Authored, unique, never reused. See the module header.
    pub id: u16,
    /// Stored in `image_scenes.ritual`. Stable for ever.
    pub slug: String,
    /// What a photographer sees.
    pub title: String,
    /// Regional and transliterated spellings. Matched, never stored.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// The member of `aura_cloud::tasks::ALLOWED_RITUALS` this maps to, when phase
    /// 04's frozen vocabulary has a word for it.
    #[serde(default)]
    pub cloud_label: Option<String>,
    /// `pre`, `day` or `post`.
    #[serde(default)]
    pub phase: String,
    /// True when the rite is typically after dark.
    #[serde(default)]
    pub night: bool,
    /// Visible objects and staging. Documentation, not features.
    #[serde(default)]
    pub evidence: String,
    /// Which file declared it. Filled in by the loader, never by the file.
    #[serde(skip)]
    pub tradition: String,
}

/// One taxonomy file, as parsed.
#[derive(Debug, Clone, Deserialize)]
struct TraditionFile {
    version: u16,
    tradition: String,
    #[serde(default)]
    #[allow(dead_code)]
    title: String,
    #[serde(default, rename = "ritual")]
    rituals: Vec<Rite>,
}

/// Every rite in every loaded file, indexed three ways.
#[derive(Debug, Clone, Default)]
pub struct Taxonomy {
    by_id: BTreeMap<u16, Rite>,
    by_slug: BTreeMap<String, u16>,
    by_alias: BTreeMap<String, u16>,
    traditions: Vec<String>,
    version: u16,
}

impl Taxonomy {
    /// Load the five shipped files.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5024` when any file is malformed, when an id is out of range, or when
    /// two rites share an id or a slug.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::load(EMBEDDED)
    }

    /// The shipped taxonomy, or an empty one.
    ///
    /// For the paths where a taxonomy is a nicety rather than a requirement - a debug
    /// panel, a test that only cares about scenes. A refusal is logged rather than
    /// returned, so this cannot be the route by which a broken file reaches production
    /// unnoticed: [`Taxonomy::embedded`] is what the classifier calls, and it fails.
    #[must_use]
    pub fn embedded_or_empty() -> Self {
        match Self::embedded() {
            Ok(loaded) => loaded,
            Err(err) => {
                tracing::error!(
                    target: "scene.taxonomy",
                    code = %err.code,
                    "the shipped ritual taxonomy would not load; rituals are unavailable"
                );
                Self::default()
            }
        }
    }

    /// Load a named set of TOML documents as one table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5024`, with the file, the key and the rule that was broken.
    pub fn load(files: &[(&str, &str)]) -> Result<Self, AuraError> {
        let mut out = Self::default();

        for (name, text) in files {
            let parsed: TraditionFile = toml::from_str(text).map_err(|err| {
                errors::config_refused(name, "file", &format!("is not valid TOML: {err}"))
            })?;

            if parsed.tradition.trim().is_empty() {
                return Err(errors::config_refused(name, "tradition", "is empty"));
            }
            // The union's version is the highest file version, so editing any file
            // bumps `image_scenes.taxonomy_ver` and re-reads the slugs.
            out.version = out.version.max(parsed.version);
            out.traditions.push(parsed.tradition.clone());

            for rite in parsed.rituals {
                Self::validate(name, &rite)?;

                if let Some(existing) = out.by_id.get(&rite.id) {
                    return Err(errors::config_refused(
                        name,
                        &rite.slug,
                        &format!(
                            "reuses id {}, already taken by `{}`; the id is the ritual head's \
                             output slot and two rites cannot share one",
                            rite.id, existing.slug
                        ),
                    ));
                }
                if let Some(existing) = out.by_slug.get(&rite.slug) {
                    return Err(errors::config_refused(
                        name,
                        &rite.slug,
                        &format!("is already declared with id {existing}; declare a rite once and let the union share it"),
                    ));
                }

                for alias in &rite.aliases {
                    let key = normalise(alias);
                    // An alias collision is not an error. "mehndi" reasonably points at
                    // several rites and the first declaration wins, deterministically,
                    // because the files are loaded in a fixed order. A refusal here
                    // would make adding a tradition harder for no gain: an alias is a
                    // search convenience and is never stored.
                    out.by_alias.entry(key).or_insert(rite.id);
                }
                out.by_alias.entry(normalise(&rite.slug)).or_insert(rite.id);

                out.by_slug.insert(rite.slug.clone(), rite.id);
                out.by_id.insert(
                    rite.id,
                    Rite {
                        tradition: parsed.tradition.clone(),
                        ..rite
                    },
                );
            }
        }

        Ok(out)
    }

    /// Every rule a rite must satisfy, in the order somebody would fix them.
    fn validate(file: &str, rite: &Rite) -> Result<(), AuraError> {
        if rite.id < FIRST_ID {
            return Err(errors::config_refused(
                file,
                &rite.slug,
                "has id 0, which is reserved for `RitualId::NONE`",
            ));
        }
        if rite.id >= SLOT_COUNT {
            return Err(errors::config_refused(
                file,
                &rite.slug,
                &format!(
                    "has id {}, at or above the ritual head's {SLOT_COUNT} output slots; a new \
                     tradition needs a wider head, which is a retrain",
                    rite.id
                ),
            ));
        }
        if rite.slug.trim().is_empty() {
            return Err(errors::config_refused(file, "slug", "is empty"));
        }
        if rite
            .slug
            .chars()
            .any(|ch| !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '_')
        {
            return Err(errors::config_refused(
                file,
                &rite.slug,
                "is not snake_case ascii; the slug is a catalog value and must survive any locale",
            ));
        }
        if rite.title.trim().is_empty() {
            return Err(errors::config_refused(
                file,
                &rite.slug,
                "has no title, so a photographer would see a slug",
            ));
        }
        Ok(())
    }

    /// The taxonomy's version, written into `image_scenes.taxonomy_ver`.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many rites are declared across every file.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// True when nothing loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// The traditions that declared something.
    #[must_use]
    pub fn traditions(&self) -> &[String] {
        &self.traditions
    }

    /// One rite by its id, which is its model slot.
    #[must_use]
    pub fn get(&self, id: RitualId) -> Option<&Rite> {
        self.by_id.get(&id.get())
    }

    /// One rite by its stored slug.
    #[must_use]
    pub fn by_slug(&self, slug: &str) -> Option<&Rite> {
        self.by_slug.get(slug).and_then(|id| self.by_id.get(id))
    }

    /// One rite by slug, title or any alias, case- and space-insensitively.
    ///
    /// The route a cloud answer takes back into the taxonomy: `SegmentNaming` returns a
    /// member of phase 04's frozen `ALLOWED_RITUALS`, which is a different vocabulary,
    /// and several of those words are this product's aliases rather than its slugs.
    #[must_use]
    pub fn resolve(&self, text: &str) -> Option<&Rite> {
        self.by_alias
            .get(&normalise(text))
            .and_then(|id| self.by_id.get(id))
    }

    /// The id for a slug, for writing a stored row back into memory.
    #[must_use]
    pub fn id_of(&self, slug: &str) -> Option<RitualId> {
        self.by_slug.get(slug).copied().map(RitualId::new)
    }

    /// Every rite a tradition declared, in id order.
    ///
    /// A *declaration* question, not a permission question - see the module header.
    #[must_use]
    pub fn for_tradition(&self, tradition: &str) -> Vec<&Rite> {
        self.by_id
            .values()
            .filter(|rite| rite.tradition == tradition)
            .collect()
    }

    /// Every rite, in id order.
    #[must_use]
    pub fn all(&self) -> Vec<&Rite> {
        self.by_id.values().collect()
    }

    /// The slots a tradition may occupy, as a mask over the head's output.
    ///
    /// Section 6.1's tradition conditioning, applied at decode time rather than only at
    /// train time. A `communion` logit on a wedding whose ritual evidence has been
    /// Hindu all day is not a close second - it is a class the evidence has ruled out -
    /// and masking it is cheaper and more honest than hoping the head learned to.
    ///
    /// Slot 0 is always allowed: the abstention is available to every tradition.
    #[must_use]
    pub fn mask_for(&self, tradition: Option<&str>) -> Vec<bool> {
        let mut mask = vec![false; SLOT_COUNT as usize];
        if let Some(slot) = mask.first_mut() {
            *slot = true;
        }
        for rite in self.by_id.values() {
            let allowed = tradition.is_none_or(|want| rite.tradition == want);
            if let Some(slot) = mask.get_mut(rite.id as usize) {
                *slot = allowed;
            }
        }
        mask
    }
}

/// Lower-case, strip everything that is not a letter or a digit.
///
/// So `"Jaimala / Varmala"`, `"jaymala"` and `"jaimala_varmala"` all reach the same
/// entry. Deliberately aggressive: an alias table exists to absorb spelling, and a
/// transliteration that differs by a hyphen is the same word.
fn normalise(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
