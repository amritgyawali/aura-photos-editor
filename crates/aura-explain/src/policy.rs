//! From a calibrated number to what the product is allowed to do about it.
//!
//! Section 6.4, and it is four lines of arithmetic wrapped in a great deal of care about
//! who is allowed to change them. The arithmetic:
//!
//! ```text
//! band = first threshold the confidence clears
//! band = band.stricter()  for each risk that applies
//! ```
//!
//! ## Why the thresholds are in a file and the multipliers are not
//!
//! The thresholds are a product decision. A PM can argue that an export needs 0.995 and a
//! cull does not, and `autonomy_bands.toml` is where that argument lives with its reasons
//! written next to it.
//!
//! The *multipliers* are switches rather than numbers, and what they multiply is not
//! configurable at all: [`Risk::irreversible`] is set from
//! `DecisionKind::is_irreversible` and nothing else. A config file that could declare a
//! retouch reversible would be a config file that could grant autonomy over somebody's
//! face by editing a boolean, and the check would then be one typo away from silence.
//!
//! ## Why a raise never touches the confidence
//!
//! Because the confidence is a claim about how often the code is right, and the risk is a
//! claim about what it costs to be wrong. Multiplying the first by the second would produce
//! a number that is neither, and the panel would show a photographer a confidence that had
//! been quietly lowered to make a policy work.
//!
//! So a raise moves the *band* and leaves the number exactly as calibration produced it.
//! The Explain panel shows both, and the reason for the raise is a reason code -
//! `raised_for_must_have`, `raised_for_irreversible` - so the photographer reads why the
//! product asked rather than wondering why 0.99 was not enough.

use std::collections::BTreeMap;

use aura_core::contract::ledger::{Autonomy, DecisionKind};
use aura_core::AuraError;

use crate::errors;

/// The band table this build ships, embedded so a binary cannot disagree with itself.
const EMBEDDED: &str = include_str!("../config/autonomy_bands.toml");

/// Where the file may be overridden per installation.
///
/// Relative to the AURA configuration directory, the same shape
/// `aura_vision::face::prominence::OVERRIDE_RELATIVE_PATH` uses. An installation that wants
/// stricter bands - a studio that never lets automation touch an export - changes this file
/// rather than the binary.
pub const OVERRIDE_RELATIVE_PATH: &str = "autonomy_bands.toml";

/// The three thresholds for one decision kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bands {
    /// At or above this, the product acts in every mode.
    pub auto_at: f32,
    /// At or above this, it acts only when Zero-Touch is on.
    pub zero_touch_at: f32,
    /// At or above this, it acts and puts the decision in the review queue.
    pub suggest_at: f32,
}

impl Bands {
    /// Section 6.4's own numbers, and the fallback when a kind has no row.
    pub const DEFAULT: Self = Self {
        auto_at: 0.98,
        zero_touch_at: 0.90,
        suggest_at: 0.75,
    };

    /// The band a confidence falls in, before any risk is applied.
    #[must_use]
    pub fn band(&self, confidence: f32) -> Autonomy {
        if confidence >= self.auto_at {
            Autonomy::Auto
        } else if confidence >= self.zero_touch_at {
            Autonomy::AutoZeroTouch
        } else if confidence >= self.suggest_at {
            Autonomy::Suggest
        } else {
            Autonomy::RequireReview
        }
    }

    /// True when the three thresholds descend and all lie in `0..1`.
    ///
    /// A table whose `suggest_at` exceeded its `auto_at` would band a confidence of 0.8 as
    /// `Auto` and one of 0.99 as `Suggest`, which is not a stricter policy or a looser one -
    /// it is a policy that is wrong in both directions at once. The loader refuses it.
    #[must_use]
    pub fn is_ordered(&self) -> bool {
        (0.0..=1.0).contains(&self.auto_at)
            && (0.0..=1.0).contains(&self.zero_touch_at)
            && (0.0..=1.0).contains(&self.suggest_at)
            && self.auto_at >= self.zero_touch_at
            && self.zero_touch_at >= self.suggest_at
    }
}

impl Default for Bands {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What makes a decision worth asking about beyond its confidence.
///
/// Three flags rather than a score, because they compose by raising a band each and a score
/// would have needed a second threshold table to interpret it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Risk {
    /// The action is hard to take back. Set from `DecisionKind::is_irreversible`, never
    /// from configuration.
    pub irreversible: bool,
    /// The decision touches a part of the wedding the gallery may not be missing.
    pub must_have: bool,
    /// The confidence went through an unfitted calibration.
    pub uncalibrated: bool,
}

impl Risk {
    /// Nothing unusual about this decision.
    pub const NONE: Self = Self {
        irreversible: false,
        must_have: false,
        uncalibrated: false,
    };

    /// The risk of a decision about a must-have moment.
    #[must_use]
    pub const fn must_have() -> Self {
        Self {
            irreversible: false,
            must_have: true,
            uncalibrated: false,
        }
    }

    /// The same risk, marked as banded on an unfitted calibration.
    #[must_use]
    pub const fn with_uncalibrated(mut self, uncalibrated: bool) -> Self {
        self.uncalibrated = uncalibrated;
        self
    }

    /// The same risk, with the kind's own reversibility filled in.
    #[must_use]
    pub const fn for_kind(mut self, kind: DecisionKind) -> Self {
        self.irreversible = kind.is_irreversible();
        self
    }

    /// The reason codes that explain why a band was raised.
    ///
    /// Empty when nothing was raised. The panel shows these beside the confidence, so that
    /// a photographer who sees 0.99 and a review request reads the reason rather than
    /// guessing at one.
    #[must_use]
    pub fn reason_codes(&self, applied: &Multipliers) -> Vec<&'static str> {
        let mut codes = Vec::new();
        if self.must_have && applied.must_have {
            codes.push("raised_for_must_have");
        }
        if self.irreversible && applied.irreversible {
            codes.push("raised_for_irreversible");
        }
        if self.uncalibrated && applied.uncalibrated {
            codes.push("uncalibrated_confidence");
        }
        codes
    }
}

/// Which risk multipliers this build applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multipliers {
    /// Raise a band for an irreversible action. Section 6.4.
    pub irreversible: bool,
    /// Raise a band for a must-have moment. Section 6.4.
    pub must_have: bool,
    /// Raise a band while nothing has been calibrated. This build's own addition.
    pub uncalibrated: bool,
}

impl Default for Multipliers {
    fn default() -> Self {
        Self {
            irreversible: true,
            must_have: true,
            uncalibrated: true,
        }
    }
}

/// The band table, loaded.
#[derive(Debug, Clone)]
pub struct AutonomyPolicy {
    version: u16,
    defaults: Bands,
    per_kind: BTreeMap<DecisionKind, Bands>,
    multipliers: Multipliers,
}

impl AutonomyPolicy {
    /// The table this build ships.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5055` when the embedded table does not parse or is not ordered. Halting, and
    /// a unit test makes sure it never happens in a shipped binary - but a build that
    /// silently fell back to defaults would be a build whose autonomy came from a bug.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse(EMBEDDED)
    }

    /// A table from TOML text.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5055` when the text does not parse, a kind is unknown, or any row's
    /// thresholds do not descend.
    pub fn parse(text: &str) -> Result<Self, AuraError> {
        let file: BandFile =
            toml::from_str(text).map_err(|err| errors::bands_refused(err.to_string()))?;

        let defaults = Bands {
            auto_at: file.defaults.auto_at,
            zero_touch_at: file.defaults.zero_touch_at,
            suggest_at: file.defaults.suggest_at,
        };
        if !defaults.is_ordered() {
            return Err(errors::bands_refused(
                "the default thresholds do not descend from auto to suggest",
            ));
        }

        let mut per_kind = BTreeMap::new();
        for row in file.kind {
            let Some(kind) = DecisionKind::parse(&row.id) else {
                return Err(errors::bands_refused(format!(
                    "`{}` is not one of the six decision kinds",
                    row.id
                )));
            };
            let bands = Bands {
                auto_at: row.auto_at,
                zero_touch_at: row.zero_touch_at,
                suggest_at: row.suggest_at,
            };
            if !bands.is_ordered() {
                return Err(errors::bands_refused(format!(
                    "the `{}` thresholds do not descend from auto to suggest",
                    row.id
                )));
            }
            if row.reason.trim().is_empty() {
                return Err(errors::bands_refused(format!(
                    "the `{}` row has no written reason, and a threshold nobody can explain \
                     is a threshold nobody can defend",
                    row.id
                )));
            }
            per_kind.insert(kind, bands);
        }

        Ok(Self {
            version: file.version,
            defaults,
            per_kind,
            multipliers: Multipliers {
                irreversible: file.risk.irreversible_raises,
                must_have: file.risk.must_have_raises,
                uncalibrated: file.risk.uncalibrated_raises,
            },
        })
    }

    /// Which version of the table this is. Recorded on every decision it bands.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many kinds carry their own row.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.per_kind.len()
    }

    /// Which multipliers are switched on.
    #[must_use]
    pub const fn multipliers(&self) -> Multipliers {
        self.multipliers
    }

    /// The thresholds for one kind.
    #[must_use]
    pub fn bands(&self, kind: DecisionKind) -> Bands {
        self.per_kind.get(&kind).copied().unwrap_or(self.defaults)
    }

    /// What the product may do with one decision.
    ///
    /// The whole of section 6.4 in six lines: band the confidence, then raise it once for
    /// each risk that applies and this build honours.
    #[must_use]
    pub fn decide(&self, kind: DecisionKind, calibrated: f32, risk: Risk) -> Autonomy {
        let mut band = self.bands(kind).band(calibrated.clamp(0.0, 1.0));
        if risk.irreversible && self.multipliers.irreversible {
            band = band.stricter();
        }
        if risk.must_have && self.multipliers.must_have {
            band = band.stricter();
        }
        if risk.uncalibrated && self.multipliers.uncalibrated {
            band = band.stricter();
        }
        band
    }
}

#[derive(Debug, serde::Deserialize)]
struct BandFile {
    version: u16,
    defaults: DefaultRow,
    #[serde(default)]
    kind: Vec<KindRow>,
    #[serde(default)]
    risk: RiskRow,
}

#[derive(Debug, serde::Deserialize)]
struct DefaultRow {
    auto_at: f32,
    zero_touch_at: f32,
    suggest_at: f32,
    #[serde(default)]
    #[allow(dead_code)]
    reason: String,
}

#[derive(Debug, serde::Deserialize)]
struct KindRow {
    id: String,
    auto_at: f32,
    zero_touch_at: f32,
    suggest_at: f32,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, serde::Deserialize)]
struct RiskRow {
    #[serde(default = "yes")]
    irreversible_raises: bool,
    #[serde(default = "yes")]
    must_have_raises: bool,
    #[serde(default = "yes")]
    uncalibrated_raises: bool,
    #[serde(default)]
    #[allow(dead_code)]
    reason_irreversible: String,
    #[serde(default)]
    #[allow(dead_code)]
    reason_must_have: String,
    #[serde(default)]
    #[allow(dead_code)]
    reason_uncalibrated: String,
}

impl Default for RiskRow {
    /// Every multiplier on.
    ///
    /// The strict direction, for the reason `Autonomy::from_str_or_review` gives: a file
    /// that forgot to mention the multipliers must not thereby switch them off.
    fn default() -> Self {
        Self {
            irreversible_raises: true,
            must_have_raises: true,
            uncalibrated_raises: true,
            reason_irreversible: String::new(),
            reason_must_have: String::new(),
            reason_uncalibrated: String::new(),
        }
    }
}

const fn yes() -> bool {
    true
}
