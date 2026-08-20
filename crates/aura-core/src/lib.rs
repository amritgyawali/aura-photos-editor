#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! The vocabulary every other AURA crate speaks: the error taxonomy, typed ids,
//! the consent contract, the clock, path safety and log redaction.
//!
//! `aura-core` depends on no other workspace crate. That rule keeps the error
//! taxonomy usable everywhere without a dependency cycle.

pub mod clock;

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod colour;
    pub mod composition;
    pub mod consent;
    pub mod cull;
    pub mod emotion;
    pub mod error;
    pub mod ids;
    pub mod integrity;
    pub mod ledger;
    pub mod local;
    pub mod moment;
    pub mod people;
    pub mod priority;
    pub mod scene;
    pub mod style;
    pub mod tone;
}

/// Named error constructors, one module per domain in the code registry.
pub mod errors {
    pub mod cloud;
    pub mod db;
    pub mod gpu;
    pub mod io;
    pub mod job;
    pub mod ml;
    pub mod raw;
    pub mod render;
}

pub mod paths;
pub mod progress;
pub mod redact;

pub use contract::colour::{
    BandReading, ColourCode, ColourDecision, ColourOutline, ColourOverride, ColourReason,
    ColourService, ColourVariant, ContentBand, CurvePoint, HslAdjustments, HslBand, HslShift,
    SkinGuardReport, ToneCurve, VariantKind,
};
pub use contract::composition::{
    Box2, CompositionCode, CompositionFlags, CompositionOutline, CompositionReason,
    CompositionResult, CompositionService, CropHint, FrameEdge, HorizonSource, Joint, JointCut,
};
pub use contract::cull::{
    Coverage, CoverageReport, CullCode, CullMode, CullOutline, CullReason, CullService, Decision,
    KeepScore, MustHave, Rejected, Selected, SelectionResult,
};
pub use contract::emotion::{
    EmotionCode, EmotionOutline, EmotionReason, EmotionService, FaceExpression, GazeTarget,
    ImageEmotion, Interaction, MomentPeak, PeakKind, Preference, ReactionLink,
};
pub use contract::error::{AuraError, AuraResult, ErrorCode, Recovery, Severity};
pub use contract::ids::{
    ContentHash, DecisionId, FaceId, FileId, IdentityId, ImportId, MaskId, MomentId, PhotoId,
    ProfileId, ProjectId, RunId, SegmentId,
};
pub use contract::integrity::{
    CropRect, ExposureVerdict, EyeOpenness, EyeState, IntegrityFlags, IntegrityOutline,
    IntegrityResult, IntegrityService, MotionKind, Reason, ReasonCode,
};
pub use contract::ledger::{
    Autonomy, DecisionKind, DecisionSource, DecisionSubject, Evidence, ExplainService, Explainable,
    LedgerDecision, LedgerOutline, LedgerReason,
};
pub use contract::local::{
    BackgroundBalanceDelta, DodgeBurnMaps, FaceLightDelta, FaceShaping, FaceZone, LocalCode,
    LocalLightPlan, LocalOp, LocalOutline, LocalOverride, LocalReason, LocalService, MaskField,
    MaskKind, ShapingZone, ShineReduction, SubjectEnhanceDelta,
};
pub use contract::moment::{
    CameraId, DuplicateKind, DuplicateSet, Moment, MomentEdit, MomentOutline, MomentService,
};
pub use contract::people::{FaceRef, ImageSubjects, PeopleService, Role, SubjectHierarchy};
pub use contract::priority::Priority;
pub use contract::scene::{
    AttrFlags, ChapterId, EditIntent, RitualId, SceneId, SceneProfile, SceneResult, SceneScore,
    Segment, Source, StoryOutline, StoryService,
};
pub use contract::style::{
    BucketDiagnostic, BucketModel, CurveShift, ExtractSource, FallbackLevel, LightingBucket,
    MatchMethod, ProfileDiagnostics, ProfileStatus, SceneGroup, SkinBias, StyleAdvice, StyleBucket,
    StyleCode, StyleDelta, StyleOutline, StylePair, StyleProfile, StyleQuery, StyleReason,
    StyleService,
};
pub use contract::tone::{
    HypothesisSource, Illuminant, IlluminantKind, ReferenceFrame, SkinLocus, ToneAlternative,
    ToneCode, ToneEstimate, ToneOutline, ToneOverride, ToneReason, ToneService,
};
