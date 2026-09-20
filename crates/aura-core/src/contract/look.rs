//! FROZEN CONTRACT. A look somebody else already published, measured from finished
//! photographs and expressed as a shift away from what phases 15 and 16 would have done.
//!
//! PHASE-31 section 5 freezes [`LookProfile`], [`ReferenceReading`] and [`LookMatchReport`]
//! before any measurer exists. The file is in `aura-core` for the reason
//! [`crate::contract::style`] is: the phases that *consume* a look are 15 and 16 (which apply
//! it), 25 (which normalises a gallery that carries it), 27 (which has to be able to say why a
//! frame looks unlike the reference) and 28 (which runs the match unattended), and none of
//! them needs the folder walk, the decoder or the solver.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **This phase learns from finals with no originals, and that is a different problem from
//! phase 17's.** Phase 17 is handed a RAW and the JPEG a photographer made from it, so it can
//! ask "what did they *do*". Here there is only the JPEG. Nobody knows what the reference
//! photographer started with, what camera made it, or how much of what is on the screen is the
//! edit and how much is the light that afternoon.
//!
//! So this phase does not try to recover an edit. It measures **appearance** - where the tones
//! sit, which way the shadows and highlights lean, how much colour there is - over a whole
//! page of photographs, and it measures the same things over the photographer's own
//! photographs *after phases 15 and 16 have decided them*. The difference between those two
//! distributions is the look. That makes it a residual by construction, which is what keeps
//! phase 17's rule - "a style is a residual, and the baseline is never re-derived" - true here
//! rather than merely restated: an empty reference and a reference that already looks like the
//! baseline both produce exactly the baseline.
//!
//! It is also phase 26's rule one level up. **Match appearance, never parameters.** There is
//! nothing in this contract that reads a slider, and there could not be: the reference is a
//! JPEG on somebody's page and it has no sliders.
//!
//! ## The second thing: a page of photographs is not a labelled set
//!
//! Phase 17 buckets a pair by what phase 07 says the photograph is *of* and what phase 15 says
//! the light *was*. A reference photograph has neither. There is no catalog row, no scene
//! classification and no illuminant estimate made from a wedding's own neutrals.
//!
//! What can be measured from the pixels alone is the light, roughly, so
//! [`LookProfile::buckets`] is keyed by [`LightingBucket`] and by nothing else. **There is no
//! scene axis in this phase and no code path that could invent one.** When a look is
//! materialised into a [`crate::contract::style::StyleProfile`] the same lighting-conditioned
//! delta is written into every [`crate::contract::style::SceneGroup`], and
//! [`LookCode::SceneAxisNotLearned`] is on the wire saying so. A look that claimed to know how
//! this photographer shoots ceremonies differently from receptions would be claiming to have
//! read something that was never in the reference.
//!
//! ## The third thing: there is no skin term here, deliberately
//!
//! Every reading in [`ReferenceReading`] is a whole-frame or whole-zone statistic. Nothing in
//! this contract isolates skin, and [`LookProfile`] cannot express a skin bias - the field does
//! not exist, so a later change would have to widen a frozen shape to add one.
//!
//! That is not an omission, it is the rule phase 15 wrote and phases 16, 17 and 25 inherited:
//! **a skin target is measured, never assumed, and the schema cannot express an alternative.**
//! Finding skin in a stranger's photograph, with no face detector that works and no identity to
//! scope it to, means declaring a hue window and calling what falls inside it skin - which is
//! exactly the fixed skin constant this product has refused four times. The defence a look gets
//! instead is the expensive one: a look is applied *before* phase 16's skin guard, which
//! measures this photographer's own frame's own skin through the real renderer and attenuates
//! or withdraws the colour half. ADR-0063 decision 3.
//!
//! ## The fourth thing: where the reference media comes from is a separate question
//!
//! [`ReferenceOrigin`] records *which page* a look was learned from and [`MediaSource`] records
//! *how the files arrived*. They are two fields because they are two facts, and on this build
//! only two of the three sources can supply bytes: see [`MediaSource::can_fetch`].

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{ProfileId, ProjectId};
use crate::contract::scene::Timestamp;
use crate::contract::style::{LightingBucket, StyleDelta};

// ---------------------------------------------------------------------------
// The numbers section 10.1 is measured against
// ---------------------------------------------------------------------------

/// The fewest reference photographs a look needs before it is worth adopting.
///
/// Twenty-four. A page's grid is three across, so twenty-four is eight rows - about what
/// somebody scrolls past while deciding they like a photographer's work. Below it the product
/// does not refuse, for phase 17's reason: refusing at twenty-three is a cliff nobody can
/// explain. It reports a strength and lets the photographer decide.
pub const USABLE_REFERENCES: u32 = 24;

/// The fewest reference photographs below which a look is refused outright.
///
/// Eight. This *is* a cliff, and it is here because the failure it prevents is not a weak
/// profile but a confident one: the median of four frames is a number with no spread underneath
/// it, and every robust statistic in [`LookAggregate`] degenerates into "whatever those four
/// photographs happened to be". Section 12's second failure mode.
pub const MIN_REFERENCES: u32 = 8;

/// At or below this many photographs a lighting bucket is called weak and named in the report.
pub const WEAK_BUCKET_REFERENCES: u32 = 6;

/// The most reference photographs one measuring pass will read.
///
/// Two thousand. A bound rather than a budget: the walk is over a folder somebody chose, and a
/// folder somebody chose can be their entire archive by accident.
pub const MAX_REFERENCES: u32 = 2_000;

/// The long edge, in pixels, every reference photograph is measured at.
///
/// Five hundred and twelve. Every reading in [`ReferenceReading`] is a distribution statistic
/// over a whole frame or a whole zone, and none of them moves meaningfully between 512 px and
/// full resolution - while decoding two thousand full-resolution JPEGs to compute a median
/// costs minutes. The number is **frozen** rather than configurable because a look measured at
/// one scale and compared against a baseline measured at another is a look whose every
/// difference includes a resampling artefact.
pub const REFERENCE_LONG_EDGE: u32 = 512;

/// The appearance distance a materialised look must reach on the photographer's own frames.
///
/// Three dE00. Above phase 17's [`crate::contract::style::MATCH_DE00_CEILING`] of 2.5 and for a
/// stated reason: phase 17 measures a fit against *the same photograph* a photographer edited,
/// and this measures a match against a *different photographer's different photographs*. The
/// two numbers are not comparable and giving this one phase 17's value would imply they were.
pub const MATCH_DE00_CEILING: f32 = 3.0;

/// Below this share of applied frames, a match figure is called partly measured and said so.
///
/// Three quarters. Above it the number describes most of the gallery and the distinction is
/// pedantic; below it a photographer reading "2.1 dE00 over sixty photographs" would be reading
/// a figure that came from twelve of them. The threshold is a *reporting* boundary and changes
/// no arithmetic - the distance is computed the same way either side of it.
pub const MEASURED_COVERAGE_FLOOR: f32 = 0.75;

/// Below this confidence a lighting bucket's delta is not applied at all.
///
/// The same value and the same argument as [`crate::contract::style::APPLY_ABOVE`]: a bucket
/// AURA is not sure about produces the global answer rather than a hedged version of its own.
pub const APPLY_ABOVE: f32 = 0.35;

/// The most a look may move a frame's exposure, in stops.
///
/// Half a stop, **below** phase 17's [`crate::contract::style::MAX_EXPOSURE_DELTA_EV`] of two
/// thirds, and the asymmetry is the point. A photographer teaching AURA from their own archive
/// is teaching it something they are entitled to be sure about. Somebody pointing at a page
/// they admire is expressing a preference about a look, and a page that reads bright may be
/// bright because that photographer shoots in Greece. ADR-0063 decision 5.
pub const MAX_EXPOSURE_DELTA_EV: f32 = 0.5;

/// The most a look may move a frame's colour temperature, in kelvin.
///
/// Six hundred, below phase 17's eight hundred, for the reason above.
pub const MAX_TEMPERATURE_DELTA_K: f32 = 600.0;

// ---------------------------------------------------------------------------
// Where the reference came from
// ---------------------------------------------------------------------------

/// How the reference photographs reached this machine.
///
/// Three, and only two of them can supply bytes on this build. [`MediaSource::PublicUrl`] is
/// declared rather than omitted for the reason `aura_generative::inpaint` declares a diffusion
/// tier it refuses on every call: a route the product does not have is a sentence a photographer
/// can read, and a variant that does not exist is a feature request nobody can see the shape of.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum MediaSource {
    /// A folder of image files the photographer pointed at.
    ///
    /// The ordinary path, and the one that works. What is in the folder is the photographer's
    /// business: photographs they saved, a page they exported, a mood board a client sent.
    #[default]
    Folder,
    /// An Instagram "Download your information" export directory.
    ///
    /// The same walk as [`MediaSource::Folder`] with the export's own layout understood, so a
    /// photographer who asked Instagram for their own data can point at what arrived rather
    /// than at the `media/posts` folder three levels inside it.
    InstagramExport,
    /// Media fetched over the network from a page address.
    ///
    /// **Not available on this build and it is not a stub.** Two separate things are missing and
    /// only one of them is code. `scripts/check-banned.sh` refuses an outbound socket outside
    /// `aura-cloud`, and `aura-cloud`'s transport has no TLS (ADR-0009), so there is no route
    /// from this process to an `https://` host at all. And reading a page's media in bulk is
    /// something the platform grants through its own API to the account that owns the page,
    /// rather than something a desktop application takes. [`LookCode::NetworkTransportAbsent`]
    /// is what a caller gets, with the two facts in it.
    PublicUrl,
}

impl MediaSource {
    /// Every source, in the order the panel offers them.
    pub const ALL: [Self; 3] = [Self::Folder, Self::InstagramExport, Self::PublicUrl];

    /// The stable slug, stored and sent on the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::InstagramExport => "instagram_export",
            Self::PublicUrl => "public_url",
        }
    }

    /// What the panel calls it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Folder => "A folder of photographs",
            Self::InstagramExport => "An Instagram data export",
            Self::PublicUrl => "Fetched from the page",
        }
    }

    /// Parse the stored slug. Unknown text is [`MediaSource::Folder`].
    #[must_use]
    pub fn from_str_or_folder(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == text)
            .unwrap_or(Self::Folder)
    }

    /// True when this build can actually obtain bytes through this source.
    ///
    /// **A const rather than a runtime probe**, because the thing that is missing is a
    /// dependency the build refuses to have rather than a service that might come back. Phase
    /// 30's `NETWORK_TRANSPORT_AVAILABLE` is the same shape and the same honesty.
    #[must_use]
    pub const fn can_fetch(self) -> bool {
        match self {
            Self::Folder | Self::InstagramExport => true,
            Self::PublicUrl => false,
        }
    }
}

impl fmt::Display for MediaSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which page or body of work a look was learned from.
///
/// Provenance, and it is a separate field from [`MediaSource`] because they answer different
/// questions. A photographer who exports their favourite page and points AURA at the folder has
/// [`MediaSource::Folder`] and an Instagram origin, and a report that could only say "a folder"
/// would have lost the fact somebody actually wants to see.
///
/// **Nothing in this type is fetched, resolved or contacted.** It is a label, it is stored
/// beside the profile, and it is rendered in the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ReferenceOrigin {
    /// An Instagram account, by handle.
    Instagram {
        /// The handle without its `@`, lowercased.
        handle: String,
    },
    /// Somewhere else on the web, by address.
    Web {
        /// The address as the photographer typed it, trimmed.
        url: String,
    },
    /// A folder, with no page behind it.
    Local {
        /// What the photographer called this reference.
        label: String,
    },
}

impl Default for ReferenceOrigin {
    fn default() -> Self {
        Self::Local {
            label: String::new(),
        }
    }
}

impl ReferenceOrigin {
    /// The longest handle Instagram issues, and the bound this parser enforces.
    pub const MAX_HANDLE: usize = 30;

    /// Read an origin out of whatever a photographer pasted into the box.
    ///
    /// Accepts `https://instagram.com/name`, `www.instagram.com/name/`, `@name` and `name`, and
    /// anything else that looks like an address becomes [`ReferenceOrigin::Web`]. **It resolves
    /// nothing and reaches nothing** - the parse is over the text, and a handle that does not
    /// exist parses exactly as well as one that does, because this build has no way to find out
    /// and pretending otherwise would put a validation tick next to an unchecked claim.
    ///
    /// # Errors
    ///
    /// [`crate::errors::ml::ML_LOOK_REFERENCE_REFUSED`] when the text is empty, or when it
    /// carries a path separator or a character Instagram does not issue in a handle.
    pub fn parse(text: &str) -> AuraResult<Self> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(crate::errors::ml::look_reference_refused(
                "the reference is empty",
            ));
        }
        if trimmed.contains('\\') {
            return Err(crate::errors::ml::look_reference_refused(
                "that looks like a file path rather than a page",
            ));
        }

        let lowered = trimmed.to_ascii_lowercase();
        let had_scheme = lowered.starts_with("https://") || lowered.starts_with("http://");
        let without_scheme = lowered
            .strip_prefix("https://")
            .or_else(|| lowered.strip_prefix("http://"))
            .unwrap_or(&lowered);
        let without_www = without_scheme
            .strip_prefix("www.")
            .unwrap_or(without_scheme);

        // An explicit `@` is a handle and nothing else, whatever is in it.
        if let Some(handle) = without_www.strip_prefix('@') {
            return Self::handle(handle);
        }

        // An Instagram address, with or without a scheme. Everything after the first path
        // segment is a post, a reel or a tab, and this phase is about the page rather than
        // about one photograph on it.
        if let Some(path) = without_www
            .strip_prefix("instagram.com/")
            .or_else(|| without_www.strip_prefix("instagr.am/"))
        {
            return Self::handle(path.split(['/', '?', '#']).next().unwrap_or_default());
        }
        if without_www == "instagram.com" || without_www == "instagr.am" {
            return Err(crate::errors::ml::look_reference_refused(
                "there is no account name in that address",
            ));
        }

        // Anything with a scheme or a path separator is an address, and its host has to look
        // like one. **This is the guard that stops a pasted file path being stored as a
        // reference**: `../../etc/passwd` has a dot and a slash and is not a page, and without
        // this check it would be recorded as one - a label, never dereferenced, but a label a
        // photographer would see and a support case would have to explain.
        if had_scheme || without_www.contains('/') {
            let host = without_www
                .split(['/', '?', '#'])
                .next()
                .unwrap_or_default();
            if !is_hostname(host) {
                return Err(crate::errors::ml::look_reference_refused(
                    "that looks like a file path rather than a page",
                ));
            }
            return Ok(Self::Web {
                url: trimmed.to_string(),
            });
        }

        // Bare text with no scheme and no separator is what somebody types when they mean an
        // account name. A handle may contain dots, so there is no way to tell `some.body` from
        // a domain and no reason to try: the value is a label either way, and reading it as a
        // handle is what the box asks for.
        Self::handle(without_www)
    }

    /// One validated handle.
    fn handle(text: &str) -> AuraResult<Self> {
        let handle = text.trim().trim_start_matches('@').to_ascii_lowercase();
        if handle.is_empty() {
            return Err(crate::errors::ml::look_reference_refused(
                "there is no account name in that address",
            ));
        }
        if handle.len() > Self::MAX_HANDLE {
            return Err(crate::errors::ml::look_reference_refused(
                "that account name is longer than Instagram issues",
            ));
        }
        if !handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        {
            return Err(crate::errors::ml::look_reference_refused(
                "that account name has characters Instagram does not issue",
            ));
        }
        Ok(Self::Instagram { handle })
    }

    /// What the report calls this reference.
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Self::Instagram { handle } => format!("@{handle}"),
            Self::Web { url } => url.clone(),
            Self::Local { label } if label.is_empty() => "A folder of photographs".to_string(),
            Self::Local { label } => label.clone(),
        }
    }

    /// The stable slug the catalog stores.
    #[must_use]
    pub fn as_key(&self) -> String {
        match self {
            Self::Instagram { handle } => format!("instagram:{handle}"),
            Self::Web { url } => format!("web:{url}"),
            Self::Local { label } => format!("local:{label}"),
        }
    }

    /// Parse the stored key back. Anything unrecognised is a local reference with the text as
    /// its label, for the reason every `from_str_or_*` in this product gives: a catalog written
    /// by a newer build must open.
    #[must_use]
    pub fn from_key(text: &str) -> Self {
        match text.split_once(':') {
            Some(("instagram", handle)) => Self::Instagram {
                handle: handle.to_string(),
            },
            Some(("web", url)) => Self::Web {
                url: url.to_string(),
            },
            Some(("local", label)) => Self::Local {
                label: label.to_string(),
            },
            _ => Self::Local {
                label: text.to_string(),
            },
        }
    }
}

/// True when this text is shaped like a hostname: dot-separated labels of alphanumerics and
/// hyphens, at least two of them, none empty.
///
/// Deliberately not a URL parser. The only question being asked is "is this a page address or a
/// file path", the answer is used to pick between a label and a refusal, and a real parser here
/// would be a dependency and a much larger surface for the sake of a yes or no.
fn is_hostname(text: &str) -> bool {
    let labels: Vec<&str> = text.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

impl fmt::Display for ReferenceOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.title())
    }
}

// ---------------------------------------------------------------------------
// What one reference photograph reads as
// ---------------------------------------------------------------------------

/// Where the tones sit in one photograph, as seven quantiles of its luminance.
///
/// Quantiles rather than a mean and a standard deviation, because a look is mostly a statement
/// about the ends: "my blacks are lifted" and "I protect my highlights" are both invisible in a
/// mean and both obvious in `p01` and `p99`. The seven are the five
/// [`crate::contract::style::CurveShift::ANCHORS`] a style delta can actually move, plus the two
/// outer ones a curve cannot move but a black and white point can.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ToneLandmarks {
    /// The 1st percentile of luminance, `0..1`. Where the darkest real detail sits.
    pub p01: f32,
    /// The 5th percentile.
    pub p05: f32,
    /// The 25th percentile.
    pub p25: f32,
    /// The median.
    pub p50: f32,
    /// The 75th percentile.
    pub p75: f32,
    /// The 95th percentile.
    pub p95: f32,
    /// The 99th percentile. Where the brightest real detail sits.
    pub p99: f32,
}

impl ToneLandmarks {
    /// The seven, in ascending order.
    #[must_use]
    pub const fn as_array(&self) -> [f32; 7] {
        [
            self.p01, self.p05, self.p25, self.p50, self.p75, self.p95, self.p99,
        ]
    }

    /// Build from an array in ascending order.
    #[must_use]
    pub const fn from_array(values: [f32; 7]) -> Self {
        Self {
            p01: values[0],
            p05: values[1],
            p25: values[2],
            p50: values[3],
            p75: values[4],
            p95: values[5],
            p99: values[6],
        }
    }

    /// The interquartile spread, which is what "contrast" means in this phase.
    ///
    /// Deliberately not the full range: `p99 - p01` is dominated by one specular highlight and
    /// one unlit doorway, and two photographs with identical midtone contrast can differ by a
    /// third in it. Phase 22's rule - a threshold on a measurement is a statement about the
    /// instrument - applied to the measurement itself.
    #[must_use]
    pub fn midtone_spread(&self) -> f32 {
        (self.p75 - self.p25).max(0.0)
    }
}

/// Which way one tonal zone of a photograph leans in colour.
///
/// CIELAB `a*` and `b*` means over the pixels in that zone. Two numbers rather than a hue and a
/// chroma because the arithmetic that follows is all differences and averages, and a hue is a
/// circular quantity that cannot be averaged without an argument about where to cut it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ZoneTint {
    /// Mean `a*`: positive toward magenta, negative toward green.
    pub a: f32,
    /// Mean `b*`: positive toward yellow, negative toward blue.
    pub b: f32,
}

impl ZoneTint {
    /// The distance between two tints, in `a*b*` units.
    #[must_use]
    pub fn distance(self, other: Self) -> f32 {
        ((self.a - other.a).powi(2) + (self.b - other.b).powi(2)).sqrt()
    }

    /// True when this zone is not tinted either way.
    #[must_use]
    pub fn is_neutral(self) -> bool {
        self.a.abs() < 1e-3 && self.b.abs() < 1e-3
    }
}

/// What one of the eight hue bands does in a photograph.
///
/// The bands are [`crate::contract::colour::HslBand`]'s, so a shift solved from these readings
/// lands in the recipe's own vocabulary rather than in a second one that has to be mapped.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct BandReading {
    /// What fraction of the frame's coloured pixels fall in this band, `0..1`.
    ///
    /// The weight every difference in this band is taken at. A band holding two per cent of a
    /// photograph is a band whose median chroma is four pixels of somebody's tie.
    pub share: f32,
    /// Mean chroma of the pixels in this band, `0..1`.
    pub chroma: f32,
    /// Mean luminance of the pixels in this band, `0..1`.
    pub luma: f32,
}

/// Everything one reference photograph says about a look.
///
/// **Every field is a distribution statistic over the whole frame or over a zone of it.** There
/// is no field here that isolates a person, a face or a region, which is the structural half of
/// this phase's skin defence - see this module's header, third thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReferenceReading {
    /// What this reading is of: a content hash, so two copies of one photograph read once.
    pub key: String,
    /// Where the tones sit.
    pub tone: ToneLandmarks,
    /// How the darkest quarter leans.
    pub shadow: ZoneTint,
    /// How the middle half leans.
    pub mid: ZoneTint,
    /// How the brightest quarter leans.
    pub high: ZoneTint,
    /// Median chroma over the whole frame, `0..1`.
    pub chroma_p50: f32,
    /// 90th-percentile chroma, `0..1`. What the most saturated real content does.
    pub chroma_p90: f32,
    /// The eight hue bands, in [`crate::contract::colour::HslBand::ALL`] order.
    pub bands: [BandReading; 8],
    /// The colour temperature this frame *renders* at, in kelvin.
    ///
    /// An appearance reading and **not** an illuminant estimate. Phase 15 asks what light was in
    /// the room, from a wedding's own neutrals; this asks what the finished photograph looks
    /// like, which is the light and the edit together and cannot be separated from one JPEG.
    pub rendered_cct_k: f32,
    /// Which lighting bucket this frame was sorted into.
    pub lighting: LightingBucket,
    /// How sure the lighting call is, `0..1`. Invariant 2.
    pub lighting_confidence: f32,
}

// ---------------------------------------------------------------------------
// What a page of them says
// ---------------------------------------------------------------------------

/// The robust middle of many [`ReferenceReading`]s, and how much they disagreed.
///
/// Medians and median absolute deviations rather than means and standard deviations, for the
/// reason phase 25's `stats.rs` gives: a page has a black-and-white frame on it, and one
/// monochrome photograph moves a mean chroma by a tenth while moving a median by nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LookAggregate {
    /// How many readings went in.
    pub samples: u32,
    /// Median tone landmarks.
    pub tone: ToneLandmarks,
    /// Median absolute deviation of the median landmark, which is the spread this look has.
    pub tone_spread: f32,
    /// Median shadow tint.
    pub shadow: ZoneTint,
    /// Median midtone tint.
    pub mid: ZoneTint,
    /// Median highlight tint.
    pub high: ZoneTint,
    /// Median of the per-frame median chroma.
    pub chroma_p50: f32,
    /// Median of the per-frame 90th-percentile chroma.
    pub chroma_p90: f32,
    /// Median band readings.
    pub bands: [BandReading; 8],
    /// Median rendered colour temperature.
    pub rendered_cct_k: f32,
}

impl LookAggregate {
    /// True when there is enough here to solve a delta from.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.samples > 0
    }

    /// True when this aggregate is weak enough to be named in the report.
    #[must_use]
    pub const fn is_weak(&self) -> bool {
        self.samples <= WEAK_BUCKET_REFERENCES
    }
}

/// One lighting bucket of a look: what the reference did, what AURA would have done, and the
/// difference between them.
///
/// **All three, stored together and on the wire.** A delta on its own is a number a
/// photographer cannot argue with; a delta beside the two aggregates it is the difference of is
/// a claim somebody can check. Phase 16's rule - a guarantee is measured, not asserted - has a
/// second half that this phase leans on: the measurement is kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LookBucket {
    /// What light this bucket is about.
    pub lighting: LightingBucket,
    /// What the reference photographs in this bucket look like.
    pub reference: LookAggregate,
    /// What the photographer's own frames in this bucket look like after phases 15 and 16.
    pub baseline: LookAggregate,
    /// The shift, already bounded. Added to phases 15 and 16's answers, never replacing them.
    pub delta: StyleDelta,
    /// How sure this bucket's delta is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// Why, strongest doubt first.
    pub reasons: Vec<LookReason>,
}

/// The honest report about one look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LookDiagnostics {
    /// Reference photographs the walk found.
    pub found: u32,
    /// Reference photographs that read successfully.
    pub measured: u32,
    /// Reference photographs that were refused, each with a written reason.
    pub refused: u32,
    /// The photographer's own frames the baseline was measured over.
    pub baseline_frames: u32,
    /// Lighting buckets with at least one reference photograph in them.
    pub buckets_populated: u32,
    /// Lighting buckets at or below [`WEAK_BUCKET_REFERENCES`].
    pub buckets_weak: u32,
    /// How strong this look is, `0..1`: what the panel puts on the bar.
    ///
    /// Sample count against [`USABLE_REFERENCES`], multiplied by how much of the
    /// photographer's own work the baseline covered. Both halves matter and a product that
    /// reported only the first would call a look strong when it had nothing to be a residual
    /// *from*.
    pub strength: f32,
    /// One sentence a photographer reads, assembled from a closed vocabulary.
    ///
    /// **Rendered from codes, never stored as prose.** Phase 27's rule, and the reason it is a
    /// field here rather than a column in migration 31.
    pub summary: String,
    /// Why, strongest doubt first.
    pub reasons: Vec<LookReason>,
}

/// A look: what somebody else's finished photographs do, as a shift from what AURA would do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookProfile {
    /// The profile. Shares [`ProfileId`] with phase 17 because a look *becomes* a style profile.
    pub id: ProfileId,
    /// What the photographer calls it.
    pub name: String,
    /// Which page or body of work it was learned from.
    pub origin: ReferenceOrigin,
    /// How the files arrived.
    pub source: MediaSource,
    /// The lean that applies to every photograph, before any conditioning on light.
    pub global: StyleDelta,
    /// Per lighting bucket, on top of the global.
    ///
    /// **Keyed by light and by nothing else.** See this module's header, second thing.
    pub buckets: BTreeMap<LightingBucket, LookBucket>,
    /// The honest report.
    pub diagnostics: LookDiagnostics,
    /// How many reference photographs it was measured from.
    pub references: u32,
    /// When, in milliseconds since the Unix epoch.
    pub measured_at: Timestamp,
    /// The render engine the match was measured against.
    ///
    /// `aura_recipe::contract::recipe::ENGINE`. A look measured against one renderer and applied
    /// by another is a look whose measured dE00 is about a build that no longer exists.
    pub engine_ver: String,
    /// Which build's measurer and solver produced it.
    pub analysis_ver: u16,
}

impl LookProfile {
    /// An empty look: the one that changes nothing.
    ///
    /// A real state rather than an error. A page of monochrome photographs measured against a
    /// monochrome baseline is exactly this, and so is a page that happens to look like what
    /// AURA already does.
    #[must_use]
    pub fn empty(id: ProfileId, name: impl Into<String>, engine: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            origin: ReferenceOrigin::default(),
            source: MediaSource::Folder,
            global: StyleDelta::neutral(),
            buckets: BTreeMap::new(),
            diagnostics: LookDiagnostics::default(),
            references: 0,
            measured_at: 0,
            engine_ver: engine.into(),
            analysis_ver: 0,
        }
    }

    /// The delta that applies to one kind of light, and whether the bucket or the global
    /// answered.
    ///
    /// **The whole resolution rule, in one place.** Bucket, then global; the bucket's delta is
    /// *added* to the global one, and a bucket whose confidence is below [`APPLY_ABOVE`] is
    /// skipped rather than half-applied. There is no third level, because there is no scene
    /// axis in this phase.
    #[must_use]
    pub fn resolve(&self, lighting: LightingBucket) -> (StyleDelta, bool) {
        let global = self.global.clone();
        match self.buckets.get(&lighting) {
            Some(bucket) if bucket.confidence >= APPLY_ABOVE => {
                (add_deltas(&global, &bucket.delta).clamped(), true)
            }
            _ => (global.clamped(), false),
        }
    }

    /// True when this look changes nothing about any photograph.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.global.is_neutral()
            && self
                .buckets
                .values()
                .all(|bucket| bucket.delta.is_neutral() || bucket.confidence < APPLY_ABOVE)
    }
}

/// Two deltas, added.
///
/// Free-standing rather than an `impl Add` on [`StyleDelta`] because phase 17 froze that type
/// and this phase may not widen it. The arithmetic is the same addition
/// [`LookProfile::resolve`] documents, and the result is clamped by its caller rather than
/// here - so the intermediate can exceed a bound and the thing that ships never does.
#[must_use]
pub fn add_deltas(base: &StyleDelta, extra: &StyleDelta) -> StyleDelta {
    let mut out = base.clone();
    out.exposure += extra.exposure;
    out.temperature_k += extra.temperature_k;
    out.tint += extra.tint;
    out.contrast += extra.contrast;
    out.highlights += extra.highlights;
    out.shadows += extra.shadows;
    out.whites += extra.whites;
    out.blacks += extra.blacks;
    out.vibrance += extra.vibrance;
    out.saturation += extra.saturation;

    let base_curve = base.curve_shift.as_array();
    let extra_curve = extra.curve_shift.as_array();
    let mut curve = [0.0_f32; 5];
    for (slot, (one, two)) in curve
        .iter_mut()
        .zip(base_curve.iter().zip(extra_curve.iter()))
    {
        *slot = one + two;
    }
    out.curve_shift = crate::contract::style::CurveShift::from_array(curve);

    for band in crate::contract::colour::HslBand::ALL {
        let one = base.hsl.get(band);
        let two = extra.hsl.get(band);
        out.hsl.set(
            band,
            crate::contract::colour::HslShift {
                h: one.h + two.h,
                s: one.s + two.s,
                l: one.l + two.l,
            },
        );
    }

    // Skin bias is deliberately **not** summed: this phase cannot produce one, so the only
    // value either side can carry is the other's, and adding two of somebody else's numbers
    // would be arithmetic over a field this phase has no evidence for.
    out.skin_bias = base.skin_bias;
    out.confidence = base.confidence.min(extra.confidence);
    out.samples = base.samples.saturating_add(extra.samples);
    out
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a look says what it says.
///
/// Twenty-two codes, closed. Invariant 2: nothing in this phase produces a number without
/// being able to say where it came from, and a code rather than a sentence for phase 09's
/// reason - a stored sentence is copy a release can change and a catalog full of English
/// cannot be translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookCode {
    // --- what the reference was ---
    /// The reference folder held fewer than [`MIN_REFERENCES`] readable photographs.
    TooFewReferences,
    /// The reference held fewer than [`USABLE_REFERENCES`]; the look is weak rather than refused.
    ReferencesBelowUsable,
    /// One reference file would not decode and was left out.
    ReferenceUnreadable,
    /// One reference file was not an image this build reads.
    ReferenceNotAnImage,
    /// Two reference files had the same content; the second was skipped.
    ReferenceDuplicate,
    /// The reference walk stopped at [`MAX_REFERENCES`].
    ReferenceLimitReached,

    // --- where it came from ---
    /// The page address was accepted and recorded; nothing was fetched from it.
    OriginRecordedNotFetched,
    /// Fetching media over the network is not available on this build.
    NetworkTransportAbsent,
    /// The folder was read as an Instagram export.
    InstagramExportLayout,

    // --- what could be learned ---
    /// No scene axis was learned, because a reference photograph carries no scene.
    SceneAxisNotLearned,
    /// This lighting bucket had at or below [`WEAK_BUCKET_REFERENCES`] photographs in it.
    BucketWeak,
    /// This lighting bucket had no reference photographs; it answers from the global lean.
    BucketEmpty,
    /// The reference photographs in this bucket disagreed too much to call it one look.
    BucketIncoherent,
    /// No skin term was learned, because this phase cannot isolate skin without assuming it.
    SkinNotLearned,
    /// No hue rotation was learned, for the reason [`LookCode::SkinNotLearned`] gives.
    HueRotationWithheld,

    // --- what was compared against ---
    /// The baseline was measured over the photographer's own frames.
    BaselineMeasured,
    /// There were no analysed frames to be a residual from, so the look was refused.
    BaselineAbsent,
    /// The baseline covered fewer frames than the reference held, so the comparison is thin.
    BaselineThin,

    // --- what happened when it was applied ---
    /// A solved shift hit one of this phase's bounds and was clamped.
    DeltaClamped,
    /// The match reached [`MATCH_DE00_CEILING`] on the photographer's own frames.
    MatchReached,
    /// The match did not reach [`MATCH_DE00_CEILING`]; the shift is still applied, bounded.
    MatchShort,
    /// A photographer's own edit was found on a frame and was not overwritten.
    UserEditPreserved,
    /// The match was measured over fewer frames than the look was applied to.
    ///
    /// Raised when [`LookMatchReport::measured_coverage`] is below
    /// [`MEASURED_COVERAGE_FLOOR`]. The look still applies everywhere - the global lean always
    /// resolves - but the number beside it describes only the frames in a light the reference
    /// also worked in.
    MatchPartlyMeasured,
}

impl LookCode {
    /// Every code, in the order the reference document lists them.
    pub const ALL: [Self; 23] = [
        Self::TooFewReferences,
        Self::ReferencesBelowUsable,
        Self::ReferenceUnreadable,
        Self::ReferenceNotAnImage,
        Self::ReferenceDuplicate,
        Self::ReferenceLimitReached,
        Self::OriginRecordedNotFetched,
        Self::NetworkTransportAbsent,
        Self::InstagramExportLayout,
        Self::SceneAxisNotLearned,
        Self::BucketWeak,
        Self::BucketEmpty,
        Self::BucketIncoherent,
        Self::SkinNotLearned,
        Self::HueRotationWithheld,
        Self::BaselineMeasured,
        Self::BaselineAbsent,
        Self::BaselineThin,
        Self::DeltaClamped,
        Self::MatchReached,
        Self::MatchShort,
        Self::UserEditPreserved,
        Self::MatchPartlyMeasured,
    ];

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooFewReferences => "too_few_references",
            Self::ReferencesBelowUsable => "references_below_usable",
            Self::ReferenceUnreadable => "reference_unreadable",
            Self::ReferenceNotAnImage => "reference_not_an_image",
            Self::ReferenceDuplicate => "reference_duplicate",
            Self::ReferenceLimitReached => "reference_limit_reached",
            Self::OriginRecordedNotFetched => "origin_recorded_not_fetched",
            Self::NetworkTransportAbsent => "network_transport_absent",
            Self::InstagramExportLayout => "instagram_export_layout",
            Self::SceneAxisNotLearned => "scene_axis_not_learned",
            Self::BucketWeak => "bucket_weak",
            Self::BucketEmpty => "bucket_empty",
            Self::BucketIncoherent => "bucket_incoherent",
            Self::SkinNotLearned => "skin_not_learned",
            Self::HueRotationWithheld => "hue_rotation_withheld",
            Self::BaselineMeasured => "baseline_measured",
            Self::BaselineAbsent => "baseline_absent",
            Self::BaselineThin => "baseline_thin",
            Self::DeltaClamped => "delta_clamped",
            Self::MatchReached => "match_reached",
            Self::MatchShort => "match_short",
            Self::UserEditPreserved => "user_edit_preserved",
            Self::MatchPartlyMeasured => "match_partly_measured",
        }
    }

    /// Parse the stored slug; unknown text is [`LookCode::BucketEmpty`], which is the code that
    /// claims the least.
    #[must_use]
    pub fn from_str_or_empty(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::BucketEmpty)
    }

    /// The sentence the panel renders.
    ///
    /// **Rendered from the code, never stored.** Phase 27's rule, applied in the phase where a
    /// stored sentence would be most tempting: this one is the text a photographer reads when
    /// they ask why the look did not do what they expected.
    #[must_use]
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::TooFewReferences => {
                "There were not enough readable photographs in that reference to measure a look."
            }
            Self::ReferencesBelowUsable => {
                "That reference has fewer photographs than AURA would like, so the look is a \
                 rough one."
            }
            Self::ReferenceUnreadable => "One reference photograph would not open.",
            Self::ReferenceNotAnImage => "One file in that folder was not a photograph.",
            Self::ReferenceDuplicate => "Two reference photographs were the same file.",
            Self::ReferenceLimitReached => {
                "That folder holds more photographs than AURA reads in one pass, so it used the \
                 first of them."
            }
            Self::OriginRecordedNotFetched => {
                "AURA noted which page this look is from. It did not visit it."
            }
            Self::NetworkTransportAbsent => {
                "This build cannot download photographs from a page. Point AURA at a folder of \
                 them instead."
            }
            Self::InstagramExportLayout => {
                "That folder is an Instagram data export, so AURA read the posts inside it."
            }
            Self::SceneAxisNotLearned => {
                "A reference photograph does not say whether it is a ceremony or a reception, so \
                 this look is about light rather than about subject."
            }
            Self::BucketWeak => "There were only a few reference photographs in this light.",
            Self::BucketEmpty => {
                "There were no reference photographs in this light, so AURA used the overall look."
            }
            Self::BucketIncoherent => {
                "The reference photographs in this light did not agree with each other enough to \
                 call them one look."
            }
            Self::SkinNotLearned => {
                "AURA did not learn anything about skin from this reference, and it will not \
                 move anybody's."
            }
            Self::HueRotationWithheld => {
                "AURA matched how strong the colours are without rotating any of them."
            }
            Self::BaselineMeasured => "The look is the difference from your own photographs.",
            Self::BaselineAbsent => {
                "AURA has not analysed enough of your own photographs yet to tell what is this \
                 look and what is your camera."
            }
            Self::BaselineThin => {
                "AURA compared against fewer of your own photographs than it would like."
            }
            Self::DeltaClamped => "Part of this look was stronger than AURA will apply.",
            Self::MatchReached => "Your photographs now sit where that reference sits.",
            Self::MatchShort => "Your photographs moved toward that reference without reaching it.",
            Self::UserEditPreserved => "A change you made by hand was kept.",
            Self::MatchPartlyMeasured => {
                "Some of your photographs were made in light this reference never worked in. The \
                 look still applies to them; the figure beside it does not describe them."
            }
        }
    }

    /// True when this code is a reason a photographer should act on rather than just read.
    #[must_use]
    pub const fn is_actionable(self) -> bool {
        matches!(
            self,
            Self::TooFewReferences
                | Self::ReferencesBelowUsable
                | Self::NetworkTransportAbsent
                | Self::BaselineAbsent
                | Self::BaselineThin
                | Self::MatchPartlyMeasured
        )
    }
}

impl fmt::Display for LookCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One code with the number behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookReason {
    /// Which code.
    pub code: LookCode,
    /// The measurement that raised it, when there is one. Never a sentence.
    pub value: Option<f32>,
    /// What the value is compared against, when there is one.
    pub threshold: Option<f32>,
}

impl LookReason {
    /// A reason with no numbers.
    #[must_use]
    pub const fn bare(code: LookCode) -> Self {
        Self {
            code,
            value: None,
            threshold: None,
        }
    }

    /// A reason carrying a measurement and what it was held to.
    #[must_use]
    pub const fn measured(code: LookCode, value: f32, threshold: f32) -> Self {
        Self {
            code,
            value: Some(value),
            threshold: Some(threshold),
        }
    }

    /// A reason carrying a measurement with nothing to compare it to.
    #[must_use]
    pub const fn counted(code: LookCode, value: f32) -> Self {
        Self {
            code,
            value: Some(value),
            threshold: None,
        }
    }
}

// ---------------------------------------------------------------------------
// What happened when it was applied
// ---------------------------------------------------------------------------

/// How close one lighting bucket landed after the look was applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BucketResidual {
    /// Which light.
    pub lighting: LightingBucket,
    /// How far the photographer's own frames sat from the reference before, in dE00.
    pub before_de00: f32,
    /// How far they sit after, in dE00.
    pub after_de00: f32,
    /// How many of the photographer's frames this was measured over.
    pub frames: u32,
}

impl BucketResidual {
    /// What fraction of the gap the look actually closed, `0..1`.
    ///
    /// **Measured against what the gap was, never against the ceiling.** Phase 27's rule: a
    /// match that closed ninety per cent of a large gap and landed just outside is a match that
    /// worked, and one that landed inside because the gap was small to begin with is not a
    /// result. A gap that was already zero closes nothing and returns 1.0, because there was
    /// nothing to do and reporting a failure would be reporting the fixture.
    #[must_use]
    pub fn realised_share(&self) -> f32 {
        if self.before_de00 <= 1e-4 {
            return 1.0;
        }
        ((self.before_de00 - self.after_de00) / self.before_de00).clamp(0.0, 1.0)
    }

    /// True when this bucket reached [`MATCH_DE00_CEILING`].
    #[must_use]
    pub fn reached(&self) -> bool {
        self.after_de00 <= MATCH_DE00_CEILING
    }
}

/// What a look did to a photographer's own photographs, measured through the real renderer.
///
/// **Measured rather than asserted**, which is phase 16's rule and the reason this type exists
/// at all. "Your gallery now looks like that page" is a claim, and the only honest version of
/// it is a number somebody can check - so the appearance distance is computed on frames that
/// have been *rendered* with the look applied, not on the parameters the solver chose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LookMatchReport {
    /// Which look.
    pub profile: ProfileId,
    /// Which project it was measured on.
    pub project: ProjectId,
    /// One row per lighting bucket that had frames on both sides.
    pub buckets: Vec<BucketResidual>,
    /// The frame-weighted appearance distance before, in dE00.
    pub before_de00: f32,
    /// The frame-weighted appearance distance after, in dE00.
    pub after_de00: f32,
    /// How many of the project's frames the look was applied to.
    pub frames: u32,
    /// How many of them the distance was actually computed over.
    ///
    /// **Two numbers rather than one, and this is the important one.** The distance is a
    /// frame-weighted mean over the buckets in [`LookMatchReport::buckets`], and a bucket only
    /// exists where the reference *and* the photographer's own work both had frames in that
    /// light. A wedding shot mostly under a light the reference page never worked in therefore
    /// produces a perfectly real dE00 that describes a small slice of it.
    ///
    /// Reporting only `frames` would say "measured over sixty photographs" about a number that
    /// came from twelve. Phase 18's rule - say what the denominator is, and put both numbers on
    /// the wire - in the place where one number would have been most flattering.
    pub measured_frames: u32,
    /// How many carried an edit the photographer made by hand, which was preserved.
    pub user_edited: u32,
    /// Why, strongest doubt first.
    pub reasons: Vec<LookReason>,
}

impl LookMatchReport {
    /// True when the whole match reached [`MATCH_DE00_CEILING`].
    ///
    /// Gated on [`LookMatchReport::measured_frames`] rather than on
    /// [`LookMatchReport::frames`], because a report whose distance was computed over nothing
    /// has not reached anything - and `after_de00` on an empty set of buckets is zero, which
    /// would otherwise read as a perfect match.
    #[must_use]
    pub fn reached(&self) -> bool {
        self.measured_frames > 0 && self.after_de00 <= MATCH_DE00_CEILING
    }

    /// What fraction of the applied frames the distance was measured over, `0..1`.
    ///
    /// What the panel says out loud when it is below one. A look can be applied to a frame in
    /// any light - the global lean always resolves - but it can only be *measured* where the
    /// reference had something to compare against.
    #[must_use]
    pub fn measured_coverage(&self) -> f32 {
        if self.frames == 0 {
            return 0.0;
        }
        // The same scoped allow `ProfileDiagnostics::acceptance` uses, and for the same reason:
        // both counts are frame counts, a wedding that reached 2^24 of them has other problems,
        // and `aura-core` carries no blanket allow because most of it has no business casting.
        #[allow(clippy::cast_precision_loss)]
        {
            (self.measured_frames as f32 / self.frames as f32).clamp(0.0, 1.0)
        }
    }

    /// What fraction of the overall gap the look closed, `0..1`.
    #[must_use]
    pub fn realised_share(&self) -> f32 {
        if self.before_de00 <= 1e-4 {
            return 1.0;
        }
        ((self.before_de00 - self.after_de00) / self.before_de00).clamp(0.0, 1.0)
    }
}

/// What a caller needs to know about looks in one project, without loading any of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LookOutline {
    /// How many looks are stored.
    pub profiles: u32,
    /// Which one this project has selected, when one is.
    pub selected: Option<ProfileId>,
    /// What the selected one is called.
    pub selected_name: Option<String>,
    /// Which page the selected one came from.
    pub selected_origin: Option<ReferenceOrigin>,
    /// How many of this project's photographs the look can be applied to.
    ///
    /// The denominator is **analysed** frames rather than every photograph, for phase 18's
    /// reason: a frame phases 15 and 16 have not decided yet has no baseline to be a residual
    /// from, and counting it as a gap sends somebody looking in the wrong place. Both numbers
    /// are on the wire.
    pub appliable: u32,
    /// How many photographs the project holds.
    pub photographs: u32,
    /// How much of the project carries a phase 15 and 16 decision, `0..1`.
    pub baseline_coverage: f32,
    /// True when the network source is available on this build. False here, and on the wire.
    pub network_transport_available: bool,
}

/// A photographer overruling one thing about a look.
///
/// Three, and none of them can widen a bound: strengthening a look past
/// [`MAX_EXPOSURE_DELTA_EV`] is not expressible, because there is no field for it. Phase 21's
/// rule - a ceiling can be lowered by a studio and raised by nobody - and the same shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum LookOverride {
    /// Rename a look.
    Rename {
        /// What to call it now.
        name: String,
    },
    /// Apply a look at less than its measured strength, `0..=1`.
    ///
    /// **Down only.** A value above one is clamped to one on construction, and the type carries
    /// no way to express "more than the reference".
    Strength {
        /// The fraction, `0..=1`.
        fraction: f32,
    },
    /// Stop applying a look to this project.
    Clear,
}

impl LookOverride {
    /// The strength override, bounded.
    #[must_use]
    pub fn strength(fraction: f32) -> Self {
        Self::Strength {
            fraction: fraction.clamp(0.0, 1.0),
        }
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// FROZEN. The only way to ask what a reference look is and what matching it did.
///
/// Twenty-eighth service of its kind, and the first whose subject is **somebody else's work**.
/// Phase 17's `StyleService` answers "what is *your* look"; this answers "what is *that* look",
/// and the two must not be one trait: a profile learned from a photographer's own delivered
/// archive carries evidence a profile measured off a public page does not, and a caller that
/// could not tell them apart would report a look matched from twenty-four JPEGs with the
/// confidence of one fitted from three hundred pairs.
///
/// No phase may keep its own reference measurer, its own appearance vocabulary or its own idea
/// of what matching a look means.
pub trait LookService: Send + Sync {
    /// What this project knows about looks.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn outline(&self, project: ProjectId) -> AuraResult<LookOutline>;

    /// Every stored look, newest first.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn profiles(&self) -> AuraResult<Vec<LookProfile>>;

    /// One look.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn profile(&self, id: ProfileId) -> AuraResult<Option<LookProfile>>;

    /// The shift that applies to one kind of light under this project's selected look.
    ///
    /// Returns the neutral delta when nothing is selected, which is the answer that changes
    /// nothing - the same guarantee phase 17 makes and for the same reason.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn advise(&self, project: ProjectId, lighting: LightingBucket) -> AuraResult<StyleDelta>;

    /// Select a look for a project, or clear the selection.
    ///
    /// # Errors
    ///
    /// [`crate::errors::ml::ML_LOOK_REFUSED`] when the look is not stored.
    fn select(&self, project: ProjectId, profile: Option<ProfileId>) -> AuraResult<()>;

    /// Apply one override.
    ///
    /// # Errors
    ///
    /// [`crate::errors::ml::ML_LOOK_REFUSED`] when the look is not stored.
    fn override_with(&self, project: ProjectId, change: &LookOverride) -> AuraResult<()>;

    /// The last measured match report for a project, when there is one.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn match_report(&self, project: ProjectId) -> AuraResult<Option<LookMatchReport>>;

    /// Delete a look. A project that had it selected falls back to no look, which is the
    /// baseline.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn forget(&self, id: ProfileId) -> AuraResult<()>;
}

/// The blanket the app uses to hold one behind an `Arc`.
impl fmt::Debug for dyn LookService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LookService")
    }
}

/// Assert at compile time that a look can never carry a skin instruction.
///
/// A function rather than a comment, for the reason phase 25's schema scan is a test rather
/// than a paragraph: the claim in this module's header is that [`LookProfile`] cannot express a
/// skin bias, and the way that stays true is that something fails to build when it stops being
/// so. [`StyleDelta::skin_bias`] exists because phase 17 can fill it honestly; this returns the
/// value this phase is allowed to put there, and there is exactly one.
#[must_use]
pub fn look_skin_bias() -> crate::contract::style::SkinBias {
    crate::contract::style::SkinBias::default()
}

/// The error a caller gets for a source this build cannot fetch through.
///
/// Free-standing so the panel, the command and the gate all render the same two facts, and so
/// there is exactly one place that decides what "not available" means.
///
/// # Errors
///
/// Always. That is the point.
#[must_use]
pub fn refuse_fetch(source: MediaSource) -> AuraError {
    crate::errors::ml::look_reference_refused(format!(
        "{} is not available on this build: outbound network access is confined to the cloud \
         gateway and that transport has no TLS, and reading a page's media in bulk is something \
         the platform grants to the account that owns it. Point AURA at a folder of photographs \
         instead.",
        source.title()
    ))
}
