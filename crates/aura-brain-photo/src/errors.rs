//! The ten failures this crate can have, and what each one falls back to.
//!
//! Two phases live here now - phase 09's frame integrity and phase 11's composition -
//! and each has the same five shapes, which is the point rather than a coincidence. Both
//! are sets of decisions over stored numbers and measured pixels judged against a
//! versioned table a product manager owns, so both fail in the same five ways: version
//! drift, a refused edit, one item that could not be done, a refused config file, and a
//! config file that is missing a row rather than broken.
//!
//! The two halves are `AURA-ML-5033` to `AURA-ML-5037` and `AURA-ML-5043` to
//! `AURA-ML-5047`, in that order, and the parallel between the two blocks is exact.
//!
//! ## The phase 09 half
//!
//! **`AURA-ML-5036` halts.** It is the only phase 09 code that stops anything, and it
//! fires when the camera calibration table is refused. Every other failure degrades
//! into a wedding that is still usable: a frame with no verdict, a body judged by the
//! fallback, a dismissal that did not take. A **half-loaded calibration table** is
//! different in kind. Sharpness is normalised by the expected MTF50 for the body, noise
//! by the read noise, and the exposure verdict decided against the measured headroom -
//! so a table that loaded nine bodies out of twenty judges a two-camera wedding by two
//! standards and produces a review queue sorted by which camera took the frame. That
//! looks like "the product hates this camera" and nothing like a config error, which is
//! precisely the class of failure invariant 9 exists to forbid. The loader refuses and
//! leaves the previous table in place.
//!
//! `AURA-ML-5037` is the one that is *expected*. A new body ships every few months and
//! this product cannot have measured it in advance; the code exists so that a wedding
//! judged against a guessed baseline says so, in the panel and in the telemetry, rather
//! than looking like a wedding judged against a measured one.
//!
//! ## The phase 11 half
//!
//! **`AURA-ML-5046` halts**, for `AURA-ML-5036`'s argument moved one phase along. A
//! half-loaded rule table judges the ceremony against measured headroom bands and the
//! reception against neutral ones, and the resulting review queue is sorted by which half
//! of the day a frame came from.
//!
//! `AURA-ML-5047` is the one that is *expected*, and it is the counterpart of
//! `AURA-ML-5037` rather than of `AURA-ML-5023`: a scene arrives in the taxonomy before
//! anybody has written its framing bands, exactly as a camera body ships before anybody
//! has measured its MTF50. The difference from `AURA-ML-5037` is what is substituted -
//! neutral bands rather than a cautious baseline - and it is why the confidence cost is a
//! flat 0.08 here and a per-row penalty there.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored verdicts came from different heads, different arithmetic or a different
/// calibration table.
pub const ML_INTEGRITY_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5033");
/// A dismissal was refused.
pub const ML_INTEGRITY_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5034");
/// One photograph could not be checked.
pub const ML_INTEGRITY_FAILED: ErrorCode = ErrorCode("AURA-ML-5035");
/// The camera calibration table was refused.
pub const ML_CALIBRATION_REFUSED: ErrorCode = ErrorCode("AURA-ML-5036");
/// A camera body has no calibration row.
pub const ML_CAMERA_UNCALIBRATED: ErrorCode = ErrorCode("AURA-ML-5037");

/// Stored rows disagree with the running build about a version.
///
/// Degraded rather than fatal: the stale verdicts keep working while the affected rows
/// are re-analysed in the background, and `IntegrityOutline` reports the lowest version
/// present so a caller about to draw a conclusion finds out that the set is mixed.
///
/// All three numbers are in the message because the support engineer's first question
/// is *which* one moved, and the answer changes what has to be redone: a `calib_ver`
/// bump re-normalises numbers already measured, a `model_ver` bump re-runs the pass.
#[must_use]
pub fn integrity_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_INTEGRITY_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} verdicts are model {}/analysis {}/calibration {}; this build is model \
             {}/analysis {}/calibration {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it judges focus, motion and exposure, so it is re-checking this \
         wedding in the background. Anything you have already reviewed is kept.",
    )
    .with_context("stale_rows", rows.to_string())
    .with_context("stored_model_ver", stored.0.to_string())
    .with_context("stored_analysis_ver", stored.1.to_string())
    .with_context("stored_calib_ver", stored.2.to_string())
}

/// A dismissal was refused. Nothing was changed.
///
/// `ask_user` rather than a retry: all three refusal cases are answered by re-reading
/// the verdict and redrawing the panel, which is a thing the interface does and not a
/// thing the code can do on its own.
#[must_use]
pub fn integrity_edit_refused(what: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_INTEGRITY_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{what}: {why}"),
        "AURA could not change that technical note. Nothing was changed.",
    )
    .with_context("target", what)
}

/// One photograph could not be checked. The pass continues.
///
/// **No row is written**, which is the whole point of the code. A frame that could not
/// be analysed must never be stored as a frame with nothing wrong, because phase 12
/// reads "nothing wrong" as evidence. The absence of a row means nobody looked, and the
/// next pass tries it again - right for a transient decode failure and harmless for a
/// permanent one.
#[must_use]
pub fn integrity_failed(photo: &str, detail: &str) -> AuraError {
    AuraError::new(
        ML_INTEGRITY_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {detail}"),
        "AURA could not check one photograph for sharpness and exposure, and has left it \
         unmarked rather than guessing. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// The calibration table was refused. Nothing was loaded and nothing was changed.
///
/// The message names the file, the key and the rule, in that order, because that is the
/// order somebody fixes them in. The same shape `AURA-ML-5024` and `AURA-ML-5031` use.
#[must_use]
pub fn calibration_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_CALIBRATION_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the measurements that decide what counts as sharp on your camera, \
         so it has not marked anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A body with no row. The fallback was used and the verdict says so.
///
/// Warning rather than degraded, and the distinction from `AURA-ML-5023` - a scene with
/// no profile - is worth stating: a missing scene profile substitutes a *neutral*
/// judgement, whereas a missing calibration row substitutes a *cautious* one and lowers
/// the confidence of every verdict it produces. The wedding is fully analysed either
/// way.
#[must_use]
pub fn camera_uncalibrated(make: &str, model: &str) -> AuraError {
    AuraError::new(
        ML_CAMERA_UNCALIBRATED,
        Severity::Warning,
        Recovery::Fallback,
        format!("no calibration row for {make} {model}; normalised by sensor resolution alone"),
        "AURA has not measured this camera model yet, so it is judging sharpness and noise more \
         cautiously on those photographs. They are all still usable.",
    )
    .with_context("make", make)
    .with_context("model", model)
}

// ---------------------------------------------------------------------------
// PHASE-11. Composition and aesthetics.
// ---------------------------------------------------------------------------

/// Stored judgements came from different heads, different arithmetic or a different rule
/// table.
pub const ML_COMPOSITION_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5043");
/// A composition dismissal was refused.
pub const ML_COMPOSITION_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5044");
/// One photograph's framing could not be judged.
pub const ML_COMPOSITION_FAILED: ErrorCode = ErrorCode("AURA-ML-5045");
/// The composition rule table was refused.
pub const ML_RULES_REFUSED: ErrorCode = ErrorCode("AURA-ML-5046");
/// A scene has no composition rule row.
pub const ML_SCENE_UNRULED: ErrorCode = ErrorCode("AURA-ML-5047");

/// Stored rows disagree with the running build about a version.
///
/// Degraded rather than fatal, exactly as `AURA-ML-5033` is: the stale judgements keep
/// working while the affected rows are re-analysed in the background, and
/// `CompositionOutline` reports the lowest version present so a caller about to draw a
/// conclusion finds out that the set is mixed.
///
/// All three numbers are in the message because the support engineer's first question is
/// *which* one moved, and the answer changes what has to be redone. A `rules_ver` bump
/// re-compares numbers that are already measured - the headroom is still the headroom,
/// only the band it is judged against moved - whereas a `model_ver` bump re-runs the
/// keypoint pass over every frame.
#[must_use]
pub fn composition_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_COMPOSITION_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} judgements are model {}/analysis {}/rules {}; this build is model \
             {}/analysis {}/rules {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it judges framing, so it is re-checking this wedding in the \
         background. Anything you have already reviewed is kept.",
    )
    .with_context("stale_rows", rows.to_string())
    .with_context("stored_model_ver", stored.0.to_string())
    .with_context("stored_analysis_ver", stored.1.to_string())
    .with_context("stored_rules_ver", stored.2.to_string())
}

/// A dismissal was refused. Nothing was changed.
///
/// `ask_user` rather than a retry, for `AURA-ML-5034`'s reason: every refusal case is
/// answered by re-reading the judgement and redrawing the panel, which is a thing the
/// interface does and not a thing the code can do on its own.
#[must_use]
pub fn composition_edit_refused(what: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_COMPOSITION_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{what}: {why}"),
        "AURA could not change that framing note. Nothing was changed.",
    )
    .with_context("target", what)
}

/// One photograph could not be judged. The pass continues.
///
/// **No row is written**, which is the whole point of the code and the same rule
/// `AURA-ML-5035` states. A frame that could not be analysed must never be stored as a
/// frame that is framed well, because phase 12 and phase 29 both read a clean judgement
/// as evidence. The absence of a row means nobody looked, and the next pass tries it
/// again.
#[must_use]
pub fn composition_failed(photo: &str, detail: &str) -> AuraError {
    AuraError::new(
        ML_COMPOSITION_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {detail}"),
        "AURA could not check the framing of one photograph, and has left it unmarked rather \
         than guessing. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// The rule table was refused. Nothing was loaded and nothing was changed.
///
/// **This halts**, and it is the second code in this crate that does. The argument is
/// `AURA-ML-5036`'s, moved one phase along: a half-loaded rule table would judge the
/// ceremony against measured bands and the reception against neutral ones, producing a
/// review queue sorted by which half of the day a frame came from. That looks like "the
/// product hates the reception" and nothing like a config error, which is precisely the
/// class of failure invariant 9 forbids. The loader refuses and leaves the previous table
/// in place.
///
/// The message names the file, the key and the rule, in that order, because that is the
/// order somebody fixes them in. The same shape `AURA-ML-5024`, `AURA-ML-5031`,
/// `AURA-ML-5036` and `AURA-ML-5039` use.
#[must_use]
pub fn rules_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_RULES_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the framing rules that decide what counts as well composed, so it \
         has not marked anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A scene with no rule row. The neutral bands were used and the judgement says so.
///
/// Warning rather than degraded, and the distinction from `AURA-ML-5037` - a camera body
/// with no calibration row - is the same one phase 09 drew against `AURA-ML-5023`: the
/// substitute here is a *neutral* rule row rather than a cautious one, so the wedding is
/// fully judged and the confidence drops by a fixed amount rather than by a per-row
/// penalty.
///
/// It is expected rather than exceptional. `scene_profiles.toml` grows a row whenever a
/// tradition is added, and a scene that reaches this analyser before the product manager
/// has written its framing bands should say so in the panel rather than quietly inherit
/// a `couple_portrait`'s headroom band.
#[must_use]
pub fn scene_unruled(scene: &str) -> AuraError {
    AuraError::new(
        ML_SCENE_UNRULED,
        Severity::Warning,
        Recovery::Fallback,
        format!("no composition rule row for `{scene}`; the neutral bands were used"),
        "AURA has no framing rules recorded for this kind of photograph yet, so it is judging \
         those ones cautiously against neutral rules. They are all still usable.",
    )
    .with_context("scene", scene)
}

// ---------------------------------------------------------------------------
// PHASE-15. Exposure and white balance.
// ---------------------------------------------------------------------------

/// Stored estimates came from different heads, different arithmetic or a different target
/// table.
pub const ML_TONE_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5060");
/// A tone override or acceptance was refused.
pub const ML_TONE_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5061");
/// One photograph's exposure and white balance could not be estimated.
pub const ML_TONE_FAILED: ErrorCode = ErrorCode("AURA-ML-5062");
/// The exposure target table was refused.
pub const ML_TARGETS_REFUSED: ErrorCode = ErrorCode("AURA-ML-5063");
/// A scene has no exposure target row.
pub const ML_SCENE_UNTARGETED: ErrorCode = ErrorCode("AURA-ML-5064");
/// No skin locus was usable, so the solve ran without the skin constraint.
pub const ML_SKIN_LOCUS_UNAVAILABLE: ErrorCode = ErrorCode("AURA-ML-5065");

/// Stored rows disagree with the running build about a version.
///
/// Degraded rather than fatal, exactly as `AURA-ML-5033` and `AURA-ML-5043` are. All three
/// numbers are in the message because the support engineer's first question is *which* one
/// moved, and here the answer changes the cost by two orders of magnitude: a `targets_ver`
/// bump re-compares measurements that already exist, whereas an `analysis_ver` bump re-reads
/// four thousand proxies.
#[must_use]
pub fn tone_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_TONE_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} estimates are model {}/analysis {}/targets {}; this build is model \
             {}/analysis {}/targets {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it sets exposure and colour, so it is re-checking this wedding \
         in the background. Anything you have already adjusted is kept.",
    )
    .with_context("stale_rows", rows.to_string())
    .with_context("stored_model_ver", stored.0.to_string())
    .with_context("stored_analysis_ver", stored.1.to_string())
    .with_context("stored_targets_ver", stored.2.to_string())
}

/// An override was refused. Nothing was recorded and nothing was rendered.
///
/// `ask_user` rather than a retry, for `AURA-ML-5034`'s reason: every refusal case is
/// answered by re-reading the estimate and redrawing the panel.
#[must_use]
pub fn tone_edit_refused(what: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_TONE_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{what}: {why}"),
        "AURA could not record that adjustment. Nothing was changed.",
    )
    .with_context("target", what)
}

/// One photograph could not be estimated. The pass continues.
///
/// **No row is written**, which is the whole point of the code and the rule `AURA-ML-5035`
/// and `AURA-ML-5045` both state. A frame stored with a neutral estimate would read to
/// phases 16, 17, 25 and 27 as "AURA decided this photograph needed nothing", and all four
/// act on that. The absence of a row means nobody looked.
#[must_use]
pub fn tone_failed(photo: &str, detail: &str) -> AuraError {
    AuraError::new(
        ML_TONE_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {detail}"),
        "AURA could not work out the exposure and colour for one photograph, and has left it \
         as the camera recorded it. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// The exposure target table was refused. Nothing was loaded and nothing was changed.
///
/// **This halts**, and it is the third code in this crate that does. The argument is
/// `AURA-ML-5036`'s and `AURA-ML-5046`'s with the stakes raised: a half-loaded target table
/// exposes the ceremony against measured bands and the reception against neutral ones, and
/// the result is a gallery whose brightness changes at a chapter boundary. Exposure is the
/// most visible decision in the product, so the most visible failure mode belongs to it.
///
/// The message names the file, the key and the rule, in that order, because that is the
/// order somebody fixes them in.
#[must_use]
pub fn targets_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_TARGETS_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the settings that decide how bright faces should be, so it has \
         not adjusted anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A scene with no target row. The neutral band was used and the estimate says so.
///
/// Warning rather than degraded, and the counterpart of `AURA-ML-5047` rather than of
/// `AURA-ML-5037`: the substitute is a *neutral* band rather than a cautious one, so the
/// wedding is fully estimated and the confidence drops by a fixed amount.
#[must_use]
pub fn scene_untargeted(scene: &str) -> AuraError {
    AuraError::new(
        ML_SCENE_UNTARGETED,
        Severity::Warning,
        Recovery::Fallback,
        format!("no exposure target row for `{scene}`; the neutral band was used"),
        "AURA has no exposure guidance recorded for this kind of photograph yet, so it is \
         adjusting those ones cautiously. They are all still usable.",
    )
    .with_context("scene", scene)
}

/// No identity had enough evidence for a skin locus, so the solve ran without one.
///
/// The one code in this block with no counterpart in phases 09 or 11, and the one worth
/// reading twice. Sections 6.2 and 6.3 both hang off a per-identity skin locus - it scores
/// the hypotheses and it is a hard constraint on the solve - and a locus below
/// `MIN_LOCUS_SAMPLES` frames does neither. A weak locus is *worse* than none, because it
/// looks like evidence, so the code fires rather than the constraint loosening quietly.
///
/// It is expected rather than exceptional in this build: phase 06's detector is a
/// placeholder and finds no faces, so every wedding raises it. That is why the message
/// points at the review queue rather than at a fix.
#[must_use]
pub fn skin_locus_unavailable(project: &str, identities: usize) -> AuraError {
    AuraError::new(
        ML_SKIN_LOCUS_UNAVAILABLE,
        Severity::Warning,
        Recovery::Fallback,
        format!(
            "no usable skin locus in project {project}; {identities} identities were seen and \
             none reached the sample floor, so white balance was solved from the light alone"
        ),
        "AURA has not yet seen enough well-lit photographs of the people here to know what \
         their skin should look like, so it has set the colour from the light alone. Those \
         photographs are worth a look.",
    )
    .with_context("project", project)
    .with_context("identities", identities.to_string())
}

// ---------------------------------------------------------------------------
// PHASE-16. Tone curves, HSL and skin protection.
// ---------------------------------------------------------------------------

/// A tone, curve or HSL override was refused.
pub const ML_COLOUR_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5067");
/// One photograph could not be graded.
pub const ML_COLOUR_FAILED: ErrorCode = ErrorCode("AURA-ML-5068");
/// The skin ceilings could not be met and the colour operations were withdrawn.
pub const ML_SKIN_GUARD_WITHDREW: ErrorCode = ErrorCode("AURA-ML-5069");
/// The tone intent table was refused.
pub const ML_INTENT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5070");
/// Stored grades came from different heads, arithmetic or intents.
pub const ML_COLOUR_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5071");

/// An override was refused. Nothing was recorded and nothing was rendered.
///
/// `ask_user` for `AURA-ML-5061`'s reason: every refusal case here is answered by re-reading
/// the decision and redrawing the panel.
#[must_use]
pub fn colour_edit_refused(what: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_COLOUR_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{what}: {why}"),
        "AURA could not record that adjustment. Nothing was changed.",
    )
    .with_context("target", what)
}

/// One photograph could not be graded. The pass continues.
///
/// **No row is written**, the rule `AURA-ML-5035`, `AURA-ML-5045` and `AURA-ML-5062` all
/// state. A frame stored with a neutral grade would read to phases 17, 25 and 27 as "AURA
/// decided this photograph needed nothing", and all three act on that.
#[must_use]
pub fn colour_failed(photo: &str, detail: &str) -> AuraError {
    AuraError::new(
        ML_COLOUR_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {detail}"),
        "AURA could not work out the contrast and colour for one photograph, and has left it \
         as the camera recorded it. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// The skin ceilings could not be met, so every colour operation was withdrawn.
///
/// **The only code in the product that fires because a guarantee could not be kept**, and it
/// is a warning rather than a failure because the photograph is still delivered: what was
/// withdrawn is the HSL, the vibrance and the saturation, not the tone, the curve or the
/// frame.
///
/// It is the visible half of section 6.3's promise. A product that silently graded on
/// anyway would be indistinguishable, frame by frame, from one that kept the promise - and
/// the difference would only show up in a gallery somebody had already delivered.
#[must_use]
pub fn skin_guard_withdrew(photo: &str, hue_deg: f32, chroma: f32) -> AuraError {
    AuraError::new(
        ML_SKIN_GUARD_WITHDREW,
        Severity::Warning,
        Recovery::Fallback,
        format!(
            "{photo}: the gentlest colour solve still moved skin {hue_deg:.2} deg and \
             {:.1} % in chroma, so every colour operation was withdrawn",
            chroma * 100.0
        ),
        "AURA could not find a colour adjustment for this photograph that left everybody's \
         skin exactly where it was, so it made none. The photograph is otherwise fully \
         edited.",
    )
    .with_context("photo", photo)
    .with_context("hue_shift_deg", format!("{hue_deg:.3}"))
}

/// The tone intent table was refused. Nothing was loaded and nothing was graded.
///
/// **This halts**, and it is the fourth code in this crate that does. `AURA-ML-5063`'s
/// argument, moved one phase along: a table that loaded the ceremony rows and dropped the
/// reception rows grades half a wedding against measured intents and half against neutral
/// ones, and the contrast visibly changes at a chapter boundary.
#[must_use]
pub fn intent_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_INTENT_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the settings that decide how much contrast each kind of \
         photograph wants, so it has not graded anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// Stored rows disagree with the running build about a version.
///
/// Degraded rather than fatal, as `AURA-ML-5033`, `AURA-ML-5043` and `AURA-ML-5060` are.
#[must_use]
pub fn colour_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_COLOUR_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} grades are model {}/analysis {}/intents {}; this build is model \
             {}/analysis {}/intents {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it grades, so it is re-checking this wedding in the \
         background. Anything you have already adjusted is kept.",
    )
    .with_context("stale_rows", rows.to_string())
    .with_context("stored_model_ver", stored.0.to_string())
    .with_context("stored_analysis_ver", stored.1.to_string())
    .with_context("stored_intent_ver", stored.2.to_string())
}
