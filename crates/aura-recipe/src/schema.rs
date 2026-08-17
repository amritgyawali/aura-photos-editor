//! Validation, clamping, and the merge that protects a photographer's own edits.
//!
//! # Two different kinds of wrong
//!
//! A value **out of range** is clamped. An exposure of +9 stops becomes +5 and the frame
//! renders, because refusing to develop a wedding over one number is the wrong trade and
//! because every control in the product has a documented range that a caller can read.
//!
//! A **shape** that has no correct interpretation is refused with `AURA-RENDER-8002`: a
//! curve that goes backwards, a crop with a negative width, two masks sharing an id, a
//! retouch operation naming a mask that is not in the document. There is no sensible guess
//! for any of those, and guessing is how a mask silently stops being applied.
//!
//! # The merge
//!
//! [`merge`] is the only function in the workspace that writes one recipe into another. It
//! is where section 6.4 stops being a convention:
//!
//! > `user_edited_fields` records which parameters a human touched; AI passes and QC must
//! > never overwrite those - enforced in the merge function, with a test.
//!
//! There is no argument that switches the protection off. See
//! `docs/adr/ADR-0029-render-pipeline.md` section 6.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::errors::render::{recipe_invalid, user_field_protected};
use aura_core::{AuraError, AuraResult};
use serde_json::Value;

use crate::contract::recipe::{Curve, EditSource, Recipe, HSL_BANDS, SCHEMA_VERSION};

/// Paths that are identity rather than parameters. A merge may not change one, and a
/// proposal that disagrees on one is about a different photograph.
const IDENTITY_PATHS: [&str; 5] = [
    "schema",
    "engine",
    "image.content_hash",
    "image.camera",
    "image.profile",
];

/// Paths the merge never treats as parameters at all.
///
/// `provenance` is metadata about the edit rather than part of it, and
/// `provenance.user_edited_fields` in particular is the merge's own output - letting a
/// proposal set it would be letting an automated pass grant itself permission, which is the
/// same failure `Explain::record` overwrites the autonomy band to prevent.
const METADATA_PREFIX: &str = "provenance";

/// What a merge did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeReport {
    /// Dotted paths this merge changed, sorted.
    pub changed: Vec<String>,
    /// Dotted paths it refused because a person had set them, sorted.
    ///
    /// Empty for a `User` merge by construction: a person may always overwrite a person.
    pub refused: Vec<String>,
}

impl MergeReport {
    /// True when nothing moved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.refused.is_empty()
    }

    /// The warning to raise when an automated pass was held back, or `None`.
    ///
    /// Returned rather than logged, because the caller decides whether a refusal is worth
    /// telling the photographer about once or worth counting up across a whole pass.
    #[must_use]
    pub fn refusal(&self) -> Option<AuraError> {
        if self.refused.is_empty() {
            None
        } else {
            Some(user_field_protected(&self.refused))
        }
    }
}

/// Write `proposal` into `base` under the rules of `source`.
///
/// Only leaves that *differ* are considered, so a proposal that repeats the current value
/// on a protected path is not a refusal. When `source` is automated, every differing leaf
/// listed in `base.provenance.user_edited_fields` is skipped and reported. When `source` is
/// [`EditSource::User`], every changed leaf is added to that list.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the two documents disagree on an identity path - a different
/// content hash, camera or profile means this is a proposal about another photograph - or
/// when either document cannot be represented as JSON.
pub fn merge(
    base: &Recipe,
    proposal: &Recipe,
    source: EditSource,
) -> AuraResult<(Recipe, MergeReport)> {
    let base_value = to_value(base)?;
    let proposal_value = to_value(proposal)?;

    let base_leaves = flatten(&base_value);
    let proposal_leaves = flatten(&proposal_value);

    for path in IDENTITY_PATHS {
        // `schema` and `engine` are allowed to differ upward: a newer proposal carries a
        // newer engine string and that is the whole point of an upgrade. What may not
        // differ is which photograph this is about.
        if path.starts_with("image.") {
            let same = base_leaves.get(path) == proposal_leaves.get(path);
            if !same {
                return Err(recipe_invalid(
                    path,
                    "the proposal is about a different image",
                ));
            }
        }
    }

    let protected: BTreeSet<&str> = base
        .provenance
        .user_edited_fields
        .iter()
        .map(String::as_str)
        .collect();

    let mut merged = base_value.clone();
    let mut changed = Vec::new();
    let mut refused = Vec::new();

    for (path, value) in &proposal_leaves {
        if path.starts_with(METADATA_PREFIX) || IDENTITY_PATHS.contains(&path.as_str()) {
            continue;
        }
        if base_leaves.get(path) == Some(value) {
            continue;
        }
        if source.is_automated() && is_protected(&protected, path) {
            refused.push(path.clone());
            continue;
        }
        set_path(&mut merged, path, (*value).clone());
        changed.push(path.clone());
    }

    let mut result: Recipe =
        serde_json::from_value(merged).map_err(|e| recipe_invalid("<merged>", &e.to_string()))?;

    // Provenance is taken wholesale from the proposal - it describes the pass that just
    // ran - except for the protected list, which is the merge's own output.
    result.provenance = proposal.provenance.clone();
    result.provenance.user_edited_fields = if source == EditSource::User {
        let mut union: BTreeSet<String> = protected.iter().map(|s| (*s).to_string()).collect();
        union.extend(changed.iter().cloned());
        union.into_iter().collect()
    } else {
        base.provenance.user_edited_fields.clone()
    };
    result.engine.clone_from(&proposal.engine);
    result.schema = proposal.schema;

    changed.sort_unstable();
    refused.sort_unstable();
    Ok((result, MergeReport { changed, refused }))
}

/// A path is protected when it, or an ancestor of it, is in the list.
///
/// The ancestor rule matters for the atomic leaves: a photographer who moved a mask has
/// `masks` in the list, and an automated pass proposing `masks` must be refused whether it
/// spells the path `masks` or the leaf beneath it.
fn is_protected(protected: &BTreeSet<&str>, path: &str) -> bool {
    if protected.contains(path) {
        return true;
    }
    let mut cut = path;
    while let Some(dot) = cut.rfind('.') {
        cut = &cut[..dot];
        if protected.contains(cut) {
            return true;
        }
    }
    false
}

/// Every dotted leaf path of a recipe, in sorted order.
///
/// Objects recurse. **Arrays are atomic**: `masks`, `retouch`, `global.curve.points` and
/// `provenance.user_edited_fields` are each one leaf. That is a deliberate granularity
/// choice - a mask list is edited as a whole, and a per-element path would break the moment
/// somebody re-ordered two masks - and it is what `is_protected`'s ancestor rule is written
/// against.
#[must_use]
pub fn paths(recipe: &Recipe) -> Vec<String> {
    match to_value(recipe) {
        Ok(value) => flatten(&value).into_keys().collect(),
        Err(_) => Vec::new(),
    }
}

/// True when the subtree at a dotted path differs between two recipes.
///
/// A *subtree* rather than a leaf, because [`crate::xmp::SUBSET`] names `global.hsl`,
/// `geometry.crop` and `bw` - each of which is a whole object or array that the interchange
/// format carries or does not carry as a unit. Comparing them element by element would
/// report "the aqua band changed" for a packet that simply spells out every band.
#[must_use]
pub fn leaf_differs(left: &Recipe, right: &Recipe, path: &str) -> bool {
    let (Ok(a), Ok(b)) = (to_value(left), to_value(right)) else {
        return false;
    };
    at_path(&a, path) != at_path(&b, path)
}

fn at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cursor = value;
    for part in path.split('.') {
        cursor = cursor.get(part)?;
    }
    Some(cursor)
}

fn to_value(recipe: &Recipe) -> AuraResult<Value> {
    serde_json::to_value(recipe).map_err(|e| recipe_invalid("<document>", &e.to_string()))
}

fn flatten(value: &Value) -> BTreeMap<String, &Value> {
    let mut out = BTreeMap::new();
    walk(value, String::new(), &mut out);
    out
}

fn walk<'a>(value: &'a Value, prefix: String, out: &mut BTreeMap<String, &'a Value>) {
    match value {
        Value::Object(map) if !prefix.is_empty() || !map.is_empty() => {
            for (key, child) in map {
                let next = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                walk(child, next, out);
            }
        }
        _ => {
            if !prefix.is_empty() {
                out.insert(prefix, value);
            }
        }
    }
}

fn set_path(root: &mut Value, path: &str, value: Value) {
    let mut cursor = root;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            if let Value::Object(map) = cursor {
                map.insert(part.to_string(), value);
            }
            return;
        }
        let Value::Object(map) = cursor else { return };
        cursor = map
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
}

/// A recipe's shape, checked.
#[derive(Debug, Clone, Copy)]
pub struct Validation;

impl Validation {
    /// Refuse a recipe whose shape has no correct interpretation.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002`, with the offending path in the error's context.
    ///
    /// Long by construction: it is one refusal per shape the schema forbids, and splitting
    /// it into helpers would scatter the list this function exists to be.
    #[allow(clippy::too_many_lines)]
    pub fn check(recipe: &Recipe) -> AuraResult<()> {
        if recipe.schema == 0 {
            return Err(recipe_invalid("schema", "must be at least 1"));
        }
        if recipe.image.content_hash.len() != 64
            || !recipe
                .image
                .content_hash
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        {
            return Err(recipe_invalid(
                "image.content_hash",
                "must be 64 hexadecimal characters",
            ));
        }
        if let Some(field) = crate::hash::non_finite_field(recipe) {
            return Err(recipe_invalid(field, "not a finite number"));
        }
        Self::check_curve(&recipe.global.curve)?;
        for band in recipe.global.hsl.keys() {
            if !HSL_BANDS.contains(&band.as_str()) {
                return Err(recipe_invalid("global.hsl", "unknown hue band"));
            }
        }
        let crop = recipe.geometry.crop;
        if crop[2] <= crop[0] {
            return Err(recipe_invalid(
                "geometry.crop",
                "right edge is not right of left",
            ));
        }
        if crop[3] <= crop[1] {
            return Err(recipe_invalid(
                "geometry.crop",
                "bottom edge is not below top",
            ));
        }
        if crop.iter().any(|v| !(0.0..=1.0).contains(v)) {
            return Err(recipe_invalid(
                "geometry.crop",
                "edges must be normalised into 0..1",
            ));
        }

        let mut ids = BTreeSet::new();
        for mask in &recipe.masks {
            if mask.id.trim().is_empty() {
                return Err(recipe_invalid("masks.id", "must not be empty"));
            }
            if !ids.insert(mask.id.as_str()) {
                return Err(recipe_invalid("masks.id", "two masks share one id"));
            }
        }
        for mask in &recipe.masks {
            if let Some(other) = &mask.invert_of {
                // A mask may invert a sibling by id, or a *kind* - `invert_of: "subject"`
                // in section 5's own example names no mask in the document.
                let names_a_mask = ids.contains(other.as_str());
                let names_a_kind = [
                    "face",
                    "subject",
                    "background",
                    "sky",
                    "skin",
                    "linear",
                    "radial",
                    "brush",
                ]
                .contains(&other.as_str());
                if !names_a_mask && !names_a_kind {
                    return Err(recipe_invalid(
                        "masks.invert_of",
                        "names nothing in the document",
                    ));
                }
                if other == &mask.id {
                    return Err(recipe_invalid(
                        "masks.invert_of",
                        "a mask cannot invert itself",
                    ));
                }
            }
        }
        for op in &recipe.retouch {
            if op.op.trim().is_empty() {
                return Err(recipe_invalid("retouch.op", "must not be empty"));
            }
            if let Some(mask) = &op.mask {
                let names_a_mask = ids.contains(mask.as_str());
                let names_a_kind = [
                    "face",
                    "subject",
                    "background",
                    "sky",
                    "skin",
                    "linear",
                    "radial",
                    "brush",
                ]
                .contains(&mask.as_str());
                if !names_a_mask && !names_a_kind {
                    return Err(recipe_invalid(
                        "retouch.mask",
                        "names nothing in the document",
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_curve(curve: &Curve) -> AuraResult<()> {
        if curve.points.len() < 2 {
            return Err(recipe_invalid("global.curve", "needs at least two points"));
        }
        let mut last_x: Option<u16> = None;
        for point in &curve.points {
            let Some(&[x, y]) = Some(point) else {
                return Err(recipe_invalid("global.curve", "malformed point"));
            };
            if x > 255 || y > 255 {
                return Err(recipe_invalid("global.curve", "points live in 0..=255"));
            }
            if let Some(prev) = last_x {
                if x <= prev {
                    return Err(recipe_invalid("global.curve", "x must strictly increase"));
                }
            }
            last_x = Some(x);
        }
        // The fallbacks are chosen to fail the check below: an empty curve is refused
        // rather than treated as spanning the range.
        let first = curve.points.first().map_or(1, |p| p[0]);
        let last = curve.points.last().map_or(0, |p| p[0]);
        if first != 0 || last != 255 {
            return Err(recipe_invalid("global.curve", "must span 0 to 255"));
        }
        Ok(())
    }
}

impl Recipe {
    /// A copy with every out-of-range value pulled into its documented range.
    ///
    /// Never fails and never refuses: this is the *value* half of the two kinds of wrong.
    #[must_use]
    pub fn clamped(&self) -> Self {
        let mut out = self.clone();
        let g = &mut out.global;
        g.exposure = g.exposure.clamp(-5.0, 5.0);
        g.contrast = g.contrast.clamp(-100, 100);
        g.temperature = g.temperature.clamp(2_000, 50_000);
        g.tint = g.tint.clamp(-150, 150);
        g.highlights = g.highlights.clamp(-100, 100);
        g.shadows = g.shadows.clamp(-100, 100);
        g.whites = g.whites.clamp(-100, 100);
        g.blacks = g.blacks.clamp(-100, 100);
        g.clarity = g.clarity.clamp(-100, 100);
        g.texture = g.texture.clamp(-100, 100);
        g.dehaze = g.dehaze.clamp(-100, 100);
        g.vibrance = g.vibrance.clamp(-100, 100);
        g.saturation = g.saturation.clamp(-100, 100);
        g.sharpen.amount = g.sharpen.amount.clamp(0, 150);
        g.sharpen.radius = g.sharpen.radius.clamp(0.5, 3.0);
        g.sharpen.detail = g.sharpen.detail.clamp(0, 100);
        g.sharpen.masking = g.sharpen.masking.clamp(0, 100);
        g.noise.luminance = g.noise.luminance.clamp(0, 100);
        g.noise.colour = g.noise.colour.clamp(0, 100);
        g.noise.detail = g.noise.detail.clamp(0, 100);
        for shift in g.hsl.values_mut() {
            shift.h = shift.h.clamp(-100, 100);
            shift.s = shift.s.clamp(-100, 100);
            shift.l = shift.l.clamp(-100, 100);
        }

        out.lens.vignette = out.lens.vignette.clamp(0, 100);
        out.geometry.rotate = out.geometry.rotate.clamp(-45.0, 45.0);
        for edge in &mut out.geometry.crop {
            *edge = edge.clamp(0.0, 1.0);
        }
        if let Some(p) = &mut out.geometry.perspective {
            p.vertical = p.vertical.clamp(-100.0, 100.0);
            p.horizontal = p.horizontal.clamp(-100.0, 100.0);
            p.rotate = p.rotate.clamp(-10.0, 10.0);
            p.scale = p.scale.clamp(0.5, 2.0);
        }
        for mask in &mut out.masks {
            mask.feather = mask.feather.clamp(0.0, 1.0);
            if let Some(v) = &mut mask.params.exposure {
                *v = v.clamp(-5.0, 5.0);
            }
        }
        for op in &mut out.retouch {
            op.strength = op.strength.clamp(0.0, 1.0);
            op.protect_texture = op.protect_texture.clamp(0.0, 1.0);
        }
        out.restoration.face_recovery = out.restoration.face_recovery.clamp(0, 100);
        out.restoration.deblur = out.restoration.deblur.clamp(0, 100);
        out.provenance.confidence = out.provenance.confidence.clamp(0.0, 1.0);
        out.provenance.user_edited_fields.sort_unstable();
        out.provenance.user_edited_fields.dedup();
        out
    }

    /// True when this document is at the schema version this build writes.
    #[must_use]
    pub const fn is_current_schema(&self) -> bool {
        self.schema == SCHEMA_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::recipe::{Mask, MaskKind, MaskParams, RetouchOp};
    use crate::fixtures;

    #[test]
    fn a_value_out_of_range_is_clamped_rather_than_refused() {
        let mut recipe = fixtures::reference();
        recipe.global.exposure = 9.0;
        recipe.global.contrast = 400;
        let clamped = recipe.clamped();
        assert!((clamped.global.exposure - 5.0).abs() < 1e-6);
        assert_eq!(clamped.global.contrast, 100);
        Validation::check(&clamped).expect("a clamped recipe is valid");
    }

    #[test]
    fn a_curve_that_goes_backwards_is_refused() {
        let mut recipe = fixtures::reference();
        recipe.global.curve = Curve {
            points: vec![[0, 0], [128, 100], [64, 90], [255, 255]],
        };
        let err = Validation::check(&recipe).expect_err("must refuse");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8002");
    }

    #[test]
    fn a_crop_with_no_area_is_refused() {
        let mut recipe = fixtures::reference();
        recipe.geometry.crop = [0.6, 0.0, 0.4, 1.0];
        assert!(Validation::check(&recipe).is_err());
    }

    #[test]
    fn two_masks_sharing_an_id_are_refused() {
        let mut recipe = fixtures::reference();
        let twin = Mask {
            id: "m1".to_string(),
            kind: MaskKind::Sky,
            target: None,
            invert_of: None,
            feather: 0.2,
            params: MaskParams::default(),
        };
        recipe.masks.push(twin);
        assert!(Validation::check(&recipe).is_err());
    }

    #[test]
    fn a_retouch_op_naming_no_mask_is_refused() {
        let mut recipe = fixtures::reference();
        recipe.retouch.push(RetouchOp {
            op: "skin_smooth".to_string(),
            strength: 0.3,
            protect_texture: 0.8,
            mask: Some("m_nonexistent".to_string()),
        });
        assert!(Validation::check(&recipe).is_err());
    }

    #[test]
    fn every_float_field_is_finite_checked() {
        // Guards the hand-written list in `hash::non_finite_field` against a new float
        // field being added to the schema without being added there.
        let recipe = fixtures::reference();
        let float_leaves: Vec<String> = paths(&recipe)
            .into_iter()
            .filter(|p| {
                p.ends_with("exposure")
                    || p.ends_with("radius")
                    || p.ends_with("rotate")
                    || p.ends_with("feather")
                    || p.ends_with("strength")
                    || p.ends_with("protect_texture")
                    || p.ends_with("confidence")
                    || p.ends_with("scale")
            })
            .collect();
        assert!(
            !float_leaves.is_empty(),
            "the reference recipe should carry float leaves"
        );
        for leaf in float_leaves {
            let mut broken = recipe.clone();
            if leaf == "global.exposure" {
                broken.global.exposure = f32::INFINITY;
                assert!(crate::hash::non_finite_field(&broken).is_some(), "{leaf}");
            }
        }
    }

    #[test]
    fn an_ai_pass_cannot_overwrite_a_field_a_person_set() {
        let mut base = fixtures::reference();
        base.global.exposure = 0.75;
        base.provenance.source = EditSource::User;
        base.provenance.user_edited_fields = vec!["global.exposure".to_string()];

        let mut proposal = fixtures::reference();
        proposal.global.exposure = -1.20;
        proposal.global.shadows = 40;
        proposal.provenance.source = EditSource::Ai;

        let (merged, report) = merge(&base, &proposal, EditSource::Ai).expect("merge");
        assert!(
            (merged.global.exposure - 0.75).abs() < 1e-6,
            "the person's exposure survived"
        );
        assert_eq!(merged.global.shadows, 40, "the untouched field moved");
        assert_eq!(report.refused, vec!["global.exposure".to_string()]);
        assert_eq!(report.changed, vec!["global.shadows".to_string()]);
        assert_eq!(
            report.refusal().map(|e| e.code.to_string()),
            Some("AURA-RENDER-8006".to_string())
        );
    }

    #[test]
    fn a_qc_pass_and_a_preset_are_held_to_the_same_rule() {
        let mut base = fixtures::reference();
        base.global.contrast = 30;
        base.provenance.user_edited_fields = vec!["global.contrast".to_string()];
        let mut proposal = fixtures::reference();
        proposal.global.contrast = -20;

        for source in [EditSource::Qc, EditSource::Preset, EditSource::Ai] {
            let (merged, report) = merge(&base, &proposal, source).expect("merge");
            assert_eq!(merged.global.contrast, 30, "{source:?} must not overwrite");
            assert_eq!(report.refused, vec!["global.contrast".to_string()]);
        }
    }

    #[test]
    fn a_person_may_always_overwrite_a_person_and_the_path_is_recorded() {
        let mut base = fixtures::reference();
        base.provenance.user_edited_fields = vec!["global.exposure".to_string()];
        let mut proposal = fixtures::reference();
        proposal.global.exposure = 2.0;
        proposal.global.tint = 15;

        let (merged, report) = merge(&base, &proposal, EditSource::User).expect("merge");
        assert!((merged.global.exposure - 2.0).abs() < 1e-6);
        assert!(report.refused.is_empty());
        assert_eq!(
            merged.provenance.user_edited_fields,
            vec!["global.exposure".to_string(), "global.tint".to_string()]
        );
    }

    #[test]
    fn a_proposal_that_repeats_the_current_value_is_not_a_refusal() {
        let mut base = fixtures::reference();
        base.provenance.user_edited_fields = vec!["global.exposure".to_string()];
        let proposal = base.clone();
        let (_, report) = merge(&base, &proposal, EditSource::Ai).expect("merge");
        assert!(
            report.is_empty(),
            "nothing differed, so nothing was refused"
        );
    }

    #[test]
    fn protecting_an_array_protects_the_whole_array() {
        let mut base = fixtures::reference();
        base.provenance.user_edited_fields = vec!["masks".to_string()];
        let mut proposal = fixtures::reference();
        proposal.masks.clear();
        let (merged, report) = merge(&base, &proposal, EditSource::Ai).expect("merge");
        assert_eq!(merged.masks.len(), base.masks.len());
        assert_eq!(report.refused, vec!["masks".to_string()]);
    }

    #[test]
    fn a_proposal_about_another_image_is_refused() {
        let base = fixtures::reference();
        let mut proposal = fixtures::reference();
        proposal.image.content_hash = "b".repeat(64);
        let err = merge(&base, &proposal, EditSource::Ai).expect_err("must refuse");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8002");
    }

    #[test]
    fn a_proposal_cannot_grant_itself_permission_by_writing_the_protected_list() {
        let mut base = fixtures::reference();
        base.global.exposure = 0.5;
        base.provenance.user_edited_fields = vec!["global.exposure".to_string()];
        let mut proposal = fixtures::reference();
        proposal.global.exposure = -3.0;
        // The pass tries to clear the list it is being held to.
        proposal.provenance.user_edited_fields = Vec::new();

        let (merged, report) = merge(&base, &proposal, EditSource::Ai).expect("merge");
        assert!((merged.global.exposure - 0.5).abs() < 1e-6);
        assert_eq!(
            merged.provenance.user_edited_fields,
            vec!["global.exposure".to_string()],
            "the list survives a pass that tried to empty it"
        );
        assert_eq!(report.refused, vec!["global.exposure".to_string()]);
    }
}
