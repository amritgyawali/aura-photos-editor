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
    pub mod camera;
    pub mod cleanup;
    pub mod colour;
    pub mod composition;
    pub mod consent;
    pub mod cull;
    pub mod curate;
    pub mod delivery;
    pub mod emotion;
    pub mod error;
    pub mod gallery;
    pub mod geometry;
    pub mod ids;
    pub mod integrity;
    pub mod learn;
    pub mod ledger;
    pub mod local;
    pub mod micro;
    pub mod moment;
    pub mod people;
    pub mod priority;
    pub mod qc;
    pub mod restore;
    pub mod retouch;
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

pub use contract::cleanup::{
    CleanupCode, CleanupDisclosure, CleanupMethod, CleanupOutline, CleanupOverride,
    CleanupProposal, CleanupReason, CleanupService, DistractionClass, PreviewRef, SafetyCheck,
    SafetyVerdict,
};
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
pub use contract::delivery::{
    DeliveryCode, DeliveryColour, DeliveryManifest, DeliveryOutline, DeliveryReason,
    DeliveryService, Destination, ExportJob, ExportOutline, ExportService, ExportSet, ExportedFile,
    FileFormat, MetadataPolicy, NameToken, NamingTemplate, OutputSharpen, ProviderId, Resize,
    SetMapping, UploadItem, UploadProgress, UploadState,
};
pub use contract::emotion::{
    EmotionCode, EmotionOutline, EmotionReason, EmotionService, FaceExpression, GazeTarget,
    ImageEmotion, Interaction, MomentPeak, PeakKind, Preference, ReactionLink,
};
pub use contract::error::{AuraError, AuraResult, ErrorCode, Recovery, Severity};
pub use contract::gallery::{
    Bound, GalleryCode, GalleryOutline, GalleryOverride, GalleryReason, GalleryService, NodeTarget,
    NormalisationDelta, Outlier, SceneNode, SkinCorrection, SkinTarget,
};
pub use contract::geometry::{
    Aspect, CropPurpose, CropSafetyReport, CropVariant, GeometryCode, GeometryOutline,
    GeometryOverride, GeometryPlan, GeometryReason, GeometryService, Keystone, LensCorrection,
    LensSource, ProtectedKind, ProtectedRegion,
};
pub use contract::ids::{
    ContentHash, DecisionId, FaceId, FileId, IdentityId, ImportId, MaskId, MomentId, NodeId,
    PhotoId, ProfileId, ProjectId, ProposalId, RunId, SegmentId,
};
pub use contract::integrity::{
    CropRect, ExposureVerdict, EyeOpenness, EyeState, IntegrityFlags, IntegrityOutline,
    IntegrityResult, IntegrityService, MotionKind, Reason, ReasonCode,
};
pub use contract::learn::{
    AbComparison, AbRow, Aggregate, Consent, Correction, CorrectionBucket, CorrectionContext,
    HeldOut, LearnCode, LearnOutline, LearnReason, LearnService, Learnable, LearningUpdate,
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
pub use contract::micro::{
    ClothingIssue, ColourLocus, GlareMethod, MicroCode, MicroField, MicroOp, MicroOutline,
    MicroOverride, MicroPlan, MicroReason, MicroRegion, MicroService, NaturalnessGuard,
    NaturalnessReport, OpFamily,
};
pub use contract::moment::{
    CameraId, DuplicateKind, DuplicateSet, Moment, MomentEdit, MomentOutline, MomentService,
};
pub use contract::people::{FaceRef, ImageSubjects, PeopleService, Role, SubjectHierarchy};
pub use contract::priority::Priority;
pub use contract::qc::{
    CategoryTally, QcCategory, QcCode, QcOutline, QcOverride, QcReason, QcReport, QcRound,
    QcService, QcTicket, Remedy, Replacement, SolveTarget, TicketStatus,
};
pub use contract::restore::{
    ArtefactReport, DenoiseSpec, DenoiseTier, NoiseModel, RecoveredFace, RestoreCode, RestoreField,
    RestoreOutline, RestoreOverride, RestorePlan, RestoreReason, RestoreRegion, RestoreService,
    RestoreSubject, RestoreWhen, RunWhere, SharpenMask, SharpenSpec,
};
// `contract::geometry` and `contract::retouch` both name a type `ProtectedKind`, and they mean
// different things: geometry's is content a crop must not cut through - a face, a pair of hands,
// the rings - and retouch's is a feature of somebody's skin that is never removed - a mole, a
// scar, a tattoo. Neither is the wrong name in its own contract, and both contracts are frozen.
// The root re-export can only carry one of them, so it carries the one that was published first
// and retouch's arrives beside it under a qualified name. Both are reachable unaliased through
// their own module, which is how every caller in the workspace already reaches them.
pub use contract::retouch::ProtectedKind as RetouchProtectedKind;
pub use contract::retouch::{
    FreqBand, InpaintMethod, ProtectedFeature, ProtectedSource, RetouchCode, RetouchOp,
    RetouchOutline, RetouchOverride, RetouchPlan, RetouchPreset, RetouchReason, RetouchService,
    TextureReport,
};
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
