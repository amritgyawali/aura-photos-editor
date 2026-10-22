//! Explicit selection editing, using the native renderer's composed coverage.
//! Frozen selections occupy the entire uncropped frame, on a bounded 512-cell grid.
use crate::{commands::IpcResult, local_mask_commands::LocalMasksDto, AppState};
use aura_core::{progress::CancelToken, AuraError, AuraResult, PhotoId, ProjectId};
use aura_recipe::{
    local_masks::{self, Component, Mode, Source},
    schema, EditSource, Recipe,
};
use serde::Deserialize;

mod advanced;
mod advanced_geometry;
mod advanced_pixels;
mod advanced_topology;
mod authoring;
mod photographic;
mod plane;
mod refinement;
#[cfg(test)]
mod tests;

// Product limits keep authored selection processing bounded on the stored matte grid.
const MAX_RADIUS_CELLS: u32 = 32;
const MAX_EDGE_CONTRAST: u32 = 100;
const MAX_COMPONENT_AREA: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Duplicate,
    Invert,
    Union,
    Intersection,
    Subtraction,
    Freeze,
    Feather,
    Expand,
    Contract,
    EdgeContrast,
    RemoveIslands,
    FillHoles,
    EditLinear,
    EditRadial,
    EditBrush,
    EditLuminance,
    ComponentMode,
    ReorderComponent,
    ExtractComponent,
    CopyComponent,
    TransformSelection,
    BorderSelection,
    KeepLargest,
    KeepConnected,
    MedianCleanup,
    ExclusiveUnion,
    ConvexHull,
    PolygonMarquee,
    RoundedMarquee,
    SectorMarquee,
    SampleColour,
    ColourWand,
    SampleLuminance,
    SaturationRange,
    TextureRange,
    ImageEdges,
    GuidedSmooth,
    EdgeGrow,
    InteriorFalloff,
    NegativeSpace,
    Skeleton,
    SplitRegions,
    AppendStack,
    ReplaceSelection,
    SyncAdjustments,
    AlignCentroid,
    FitBounds,
    PerspectiveSelection,
    AutoThreshold,
    ChromaKey,
    VoronoiPartition,
    FilterRegions,
    HueArc,
    OpponentChroma,
    TemperatureBalance,
    ChannelClipping,
    NeutralAxis,
    LocalContrastPolarity,
    LocalEntropy,
    LaplacianDetail,
    EdgeOrientation,
    TensorCoherence,
    HarrisCorners,
    LocalRank,
    ChromaticBoundary,
    SpecularResidual,
    ShadowChromaDeviation,
    SuperellipseMarquee,
    BezierRibbon,
    StarMarquee,
    SeedField,
    BilateralSymmetry,
    PeriodicSelection,
    PolarSelection,
    CoverageQuantiles,
    HarmonicPatch,
    RegionAxisStrip,
    LabSampleDistance,
    MahalanobisColour,
    GeodesicColour,
    LocalOtsu,
    SauvolaThreshold,
    GaborResponse,
    DirectionalRidge,
    LocalSkewness,
    LocalKurtosis,
    ColourLineResidual,
    CoverageOpening,
    CoverageClosing,
    MorphologicalReconstruction,
    GrayscaleAreaOpening,
    BorderConnected,
    GeodesicRings,
    HoleBearingRegions,
    RegionalMaxima,
    SpiralMarquee,
    BarycentricTriangle,
    CoverageShear,
    CoverageTwirl,
    CoverageSpherize,
    CoverageDisplace,
    SignedDistanceSmooth,
    MedianAbsoluteDeviation,
    BinaryPatternTransitions,
    PatchAutocorrelation,
    PatchSemivariance,
    LogChromaDistance,
    EdgeNonmaximum,
    EdgeHysteresis,
    LocalMoran,
    TemplatePatchCorrelation,
    LocalHueDispersion,
    ColourPrototypePartition,
    SelectionArticulations,
    SelectionKCore,
    CoverageWatershed,
    CoverageAnisotropicDiffusion,
    CoverageCurvatureFlow,
    RegionInscribedDisk,
    DirectionalRunWidth,
    ThinPlateCoverageWarp,
    MeanValueCageWarp,
    TriangulatedCageWarp,
    MobiusCoverageWarp,
    PathNormalUnwrap,
    CoverageSeamCarve,
    LissajousRibbon,
}
impl Action {
    #[allow(clippy::too_many_lines)] // Explicit audit labels correspond one-to-one with the typed actions.
    fn label(self) -> &'static str {
        match self {
            Self::Duplicate => "Duplicate mask",
            Self::Invert => "Invert whole selection",
            Self::Union => "Union mask selections",
            Self::Intersection => "Intersect mask selections",
            Self::Subtraction => "Subtract mask selection",
            Self::Freeze => "Freeze composed selection",
            Self::Feather => "Feather selection",
            Self::Expand => "Expand selection",
            Self::Contract => "Contract selection",
            Self::EdgeContrast => "Refine selection contrast",
            Self::RemoveIslands => "Remove selection islands",
            Self::FillHoles => "Fill selection holes",
            Self::EditLinear => "Edit linear gradient geometry",
            Self::EditRadial => "Edit radial gradient geometry",
            Self::EditBrush => "Edit authored brush strokes",
            Self::EditLuminance => "Edit selection luminance range",
            Self::ComponentMode => "Change component combination",
            Self::ReorderComponent => "Reorder mask components",
            Self::ExtractComponent => "Extract mask component",
            Self::CopyComponent => "Copy component to mask",
            Self::TransformSelection => "Transform composed selection",
            Self::BorderSelection => "Select boundary band",
            Self::KeepLargest => "Keep largest selection region",
            Self::KeepConnected => "Keep seed-connected selection",
            Self::MedianCleanup => "Median selection cleanup",
            Self::ExclusiveUnion => "Exclusive union of selections",
            Self::ConvexHull => "Convex hull of selection",
            Self::PolygonMarquee => "Polygon selection",
            Self::RoundedMarquee => "Rounded rectangle selection",
            Self::SectorMarquee => "Angular sector selection",
            Self::SampleColour => "Sampled colour selection",
            Self::ColourWand => "Contiguous colour wand",
            Self::SampleLuminance => "Sampled luminance selection",
            Self::SaturationRange => "Saturation range selection",
            Self::TextureRange => "Texture range selection",
            Self::ImageEdges => "Image edge selection",
            Self::GuidedSmooth => "Image-guided mask smoothing",
            Self::EdgeGrow => "Edge-constrained selection growth",
            Self::InteriorFalloff => "Interior distance falloff",
            Self::NegativeSpace => "Select enclosed negative space",
            Self::Skeleton => "Extract selection centreline",
            Self::SplitRegions => "Split disconnected selection regions",
            Self::AppendStack => "Append selection component stack",
            Self::ReplaceSelection => "Replace destination selection",
            Self::SyncAdjustments => "Synchronize mask adjustments",
            Self::AlignCentroid => "Align selection centroids",
            Self::FitBounds => "Fit selection to bounds",
            Self::PerspectiveSelection => "Perspective selection placement",
            Self::AutoThreshold => "Automatic selection threshold",
            Self::ChromaKey => "Chroma-key selection exclusion",
            Self::VoronoiPartition => "Seed partition of selection",
            Self::FilterRegions => "Filter selection regions by shape",
            Self::HueArc => "Circular hue selection",
            Self::OpponentChroma => "Opponent colourfulness selection",
            Self::TemperatureBalance => "Red-blue balance selection",
            Self::ChannelClipping => "Channel clipping selection",
            Self::NeutralAxis => "Neutral-axis selection",
            Self::LocalContrastPolarity => "Local contrast polarity selection",
            Self::LocalEntropy => "Local entropy selection",
            Self::LaplacianDetail => "Laplacian detail selection",
            Self::EdgeOrientation => "Oriented image edge selection",
            Self::TensorCoherence => "Structure coherence selection",
            Self::HarrisCorners => "Harris corner selection",
            Self::LocalRank => "Local luminance rank selection",
            Self::ChromaticBoundary => "Chromatic boundary selection",
            Self::SpecularResidual => "Specular residual selection",
            Self::ShadowChromaDeviation => "Shadow chroma deviation selection",
            Self::SuperellipseMarquee => "Superellipse selection",
            Self::BezierRibbon => "Cubic Bezier ribbon selection",
            Self::StarMarquee => "Alternating-radius star selection",
            Self::SeedField => "Inverse-distance seed field",
            Self::BilateralSymmetry => "Bilateral symmetry selection",
            Self::PeriodicSelection => "Periodic selection tiling",
            Self::PolarSelection => "Polar selection remapping",
            Self::CoverageQuantiles => "Coverage quantile equalization",
            Self::HarmonicPatch => "Harmonic selection patch repair",
            Self::RegionAxisStrip => "Principal-axis region strip",
            Self::LabSampleDistance => "CIE Lab sampled distance",
            Self::MahalanobisColour => "Covariance colour distance",
            Self::GeodesicColour => "Geodesic source colour selection",
            Self::LocalOtsu => "Local Otsu threshold selection",
            Self::SauvolaThreshold => "Sauvola adaptive threshold selection",
            Self::GaborResponse => "Gabor frequency selection",
            Self::DirectionalRidge => "Directional ridge selection",
            Self::LocalSkewness => "Local luminance skewness",
            Self::LocalKurtosis => "Local luminance kurtosis",
            Self::ColourLineResidual => "Colour-line residual selection",
            Self::CoverageOpening => "Grayscale coverage opening",
            Self::CoverageClosing => "Grayscale coverage closing",
            Self::MorphologicalReconstruction => "Coverage opening by reconstruction",
            Self::GrayscaleAreaOpening => "Grayscale area opening",
            Self::BorderConnected => "Border-connected selection regions",
            Self::GeodesicRings => "Seed geodesic distance bands",
            Self::HoleBearingRegions => "Hole-bearing selection regions",
            Self::RegionalMaxima => "Regional coverage maxima",
            Self::SpiralMarquee => "Archimedean spiral ribbon",
            Self::BarycentricTriangle => "Barycentric triangle coverage",
            Self::CoverageShear => "Affine coverage shear",
            Self::CoverageTwirl => "Radial coverage twirl",
            Self::CoverageSpherize => "Spherical coverage remap",
            Self::CoverageDisplace => "Source-gradient coverage displacement",
            Self::SignedDistanceSmooth => "Signed-distance coverage smoothing",
            Self::MedianAbsoluteDeviation => "Robust median deviation",
            Self::BinaryPatternTransitions => "Local binary-pattern transitions",
            Self::PatchAutocorrelation => "Directional patch autocorrelation",
            Self::PatchSemivariance => "Directional semivariance",
            Self::LogChromaDistance => "Log-chromaticity sample distance",
            Self::EdgeNonmaximum => "Gradient nonmaximum suppression",
            Self::EdgeHysteresis => "Connected hysteresis edges",
            Self::LocalMoran => "Local Moran spatial association",
            Self::TemplatePatchCorrelation => "Sampled patch correlation",
            Self::LocalHueDispersion => "Circular hue dispersion",
            Self::ColourPrototypePartition => "Colour prototype partition",
            Self::SelectionArticulations => "Connectivity articulation cells",
            Self::SelectionKCore => "Selection graph core",
            Self::CoverageWatershed => "Seeded coverage watershed",
            Self::CoverageAnisotropicDiffusion => "Anisotropic coverage diffusion",
            Self::CoverageCurvatureFlow => "Coverage curvature flow",
            Self::RegionInscribedDisk => "Region maximum inscribed disk",
            Self::DirectionalRunWidth => "Directional selected run width",
            Self::ThinPlateCoverageWarp => "Thin-plate landmark deformation",
            Self::MeanValueCageWarp => "Mean-value cage deformation",
            Self::TriangulatedCageWarp => "Triangulated cage deformation",
            Self::MobiusCoverageWarp => "Conformal Mobius deformation",
            Self::PathNormalUnwrap => "Path-normal coverage unwrap",
            Self::CoverageSeamCarve => "Coverage-aware seam compression",
            Self::LissajousRibbon => "Lissajous ribbon construction",
        }
    }
    fn combines(self) -> bool {
        matches!(
            self,
            Self::Union
                | Self::Intersection
                | Self::Subtraction
                | Self::ExclusiveUnion
                | Self::CopyComponent
                | Self::AppendStack
                | Self::ReplaceSelection
                | Self::SyncAdjustments
                | Self::AlignCentroid
        )
    }
    fn parameterized(self) -> bool {
        matches!(
            self,
            Self::Feather
                | Self::Expand
                | Self::Contract
                | Self::EdgeContrast
                | Self::RemoveIslands
                | Self::FillHoles
                | Self::BorderSelection
                | Self::MedianCleanup
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub project_id: String,
    pub photo_id: String,
    pub mask_id: String,
    pub recipe_hash: String,
    pub cancel_id: String,
    pub action: Action,
    #[serde(default)]
    pub other_mask_id: Option<String>,
    #[serde(default)]
    pub value: u32,
    #[serde(default)]
    pub component_index: usize,
    #[serde(default)]
    pub target_index: usize,
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub mode: Option<Mode>,
    #[serde(default)]
    pub transform: Option<authoring::Transform>,
    #[serde(default)]
    pub point: Option<[f32; 2]>,
    #[serde(default)]
    pub advanced: Option<advanced::Options>,
}

fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("mask workflow", message);
    error.user_message = message.into();
    error
}
fn check(cancel: &CancelToken) -> AuraResult<()> {
    if cancel.is_cancelled() {
        Err(invalid("Selection editing stopped. No edit was saved."))
    } else {
        Ok(())
    }
}
fn validate(input: &Input) -> AuraResult<()> {
    let max = match input.action {
        Action::Feather | Action::Expand | Action::Contract | Action::BorderSelection => {
            MAX_RADIUS_CELLS
        }
        Action::MedianCleanup => 4,
        Action::EdgeContrast => MAX_EDGE_CONTRAST,
        Action::RemoveIslands | Action::FillHoles => MAX_COMPONENT_AREA,
        _ => 0,
    };
    if input.value > max
        || (input.action.combines()
            && input
                .other_mask_id
                .as_deref()
                .is_none_or(|id| id == input.mask_id))
    {
        return Err(invalid(
            "Choose a different second mask and a value within the displayed range.",
        ));
    }
    Ok(())
}

/// Apply one explicit operation, preserving local sliders and unrelated manual edits.
/// # Errors
/// Invalid membership, stale recipe, selection failure, cancellation, or invalid bounds.
pub fn apply(state: &AppState, input: &Input) -> IpcResult<LocalMasksDto> {
    apply_with_cancel(state, input, CancelToken::new())
}

/// Use the token registered by the desktop adapter before queueing its worker.
/// An already-cancelled token remains cancelled when the worker starts.
/// # Errors
/// The same validation, rendering and stale-recipe errors as [`apply`].
pub fn apply_with_cancel(
    state: &AppState,
    input: &Input,
    cancel: CancelToken,
) -> IpcResult<LocalMasksDto> {
    let active = cancel.clone();
    state.register_job(&input.cancel_id, cancel);
    let result = (|| -> AuraResult<LocalMasksDto> {
        validate(input)?;
        uuid::Uuid::parse_str(&input.cancel_id)
            .map_err(|_| invalid("Invalid selection editing job."))?;
        check(&active)?;
        crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
        let project =
            ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection."))?;
        let photo =
            PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photograph."))?;
        run(state, input, project, photo, &active)
    })();
    state.finish_job(&input.cancel_id);
    Ok(result?)
}
fn run(
    state: &AppState,
    input: &Input,
    project: ProjectId,
    photo: PhotoId,
    cancel: &CancelToken,
) -> AuraResult<LocalMasksDto> {
    check(cancel)?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    if aura_recipe::hash::recipe_hash(&base)? != input.recipe_hash {
        return Err(invalid(
            "The photograph changed. Reload the masks before editing its selection.",
        ));
    }
    let mut masks = local_masks::read(&base)?;
    let original = masks
        .iter()
        .find(|mask| mask.id == input.mask_id)
        .cloned()
        .ok_or_else(|| invalid("That mask no longer exists."))?;
    let mut changed = original.clone();
    let mut mattes = local_masks::read_mattes(&base)?;
    if advanced::manages(input.action) {
        changed.id = advanced::manage(state, input, photo, &base, &mut masks, &mut mattes, cancel)?;
    } else if authoring::is_authored(input.action) {
        changed.id = authoring::edit(input, &mut masks)?;
    } else if input.action == Action::Duplicate {
        if masks.len() >= local_masks::MAX_MASKS {
            return Err(invalid(
                "This photograph already has the maximum number of masks.",
            ));
        }
        changed.id = format!("mask-{}", uuid::Uuid::new_v4().simple());
        changed.name = format!(
            "{} copy",
            original.name.chars().take(20).collect::<String>()
        );
        masks.push(changed.clone());
    } else if !input.action.parameterized() || input.value > 0 {
        let selection = measured(state, input, photo, &base, cancel)?;
        let matte = selection.matte();
        let encoded =
            serde_json::to_vec(&matte).map_err(|_| invalid("Cannot save edited selection."))?;
        let id = format!("selection-{}", blake3::hash(&encoded).to_hex());
        mattes.insert(id.clone(), matte);
        changed.components = vec![Component {
            mode: Mode::Add,
            invert: false,
            source: Source::Matte {
                matte: id,
                what: "edited selection".into(),
            },
        }];
        if let Some(mask) = masks.iter_mut().find(|mask| mask.id == input.mask_id) {
            *mask = changed.clone();
        }
    }
    let mut proposal = base.clone();
    local_masks::write(&mut proposal, &masks, &mattes)?;
    if matches!(
        input.action,
        Action::Duplicate
            | Action::ExtractComponent
            | Action::SyncAdjustments
            | Action::SplitRegions
    ) {
        let mut color = aura_recipe::studio_color::read(&base)?;
        if let Some(local) = color.local.get(&input.mask_id).cloned() {
            if input.action == Action::SplitRegions {
                for mask in masks.iter().filter(|mask| {
                    !local_masks::read(&base)
                        .is_ok_and(|old| old.iter().any(|old| old.id == mask.id))
                }) {
                    color.local.insert(mask.id.clone(), local.clone());
                }
            } else {
                color.local.insert(changed.id.clone(), local);
            }
            aura_recipe::studio_color::write(&mut proposal, &color)?;
        } else if input.action == Action::SyncAdjustments {
            color.local.remove(&changed.id);
            aura_recipe::studio_color::write(&mut proposal, &color)?;
        }
    }
    let saved = save(state, input, (project, photo), &base, &proposal, cancel)?;
    Ok(LocalMasksDto {
        masks,
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &saved),
        mask_id: Some(changed.id),
        message: None,
    })
}

fn measured(
    state: &AppState,
    input: &Input,
    photo: PhotoId,
    base: &Recipe,
    cancel: &CancelToken,
) -> AuraResult<plane::Plane> {
    let render = state.render()?;
    check(cancel)?;
    let (rgb, width, height) = render.local_mask_coverage(&photo, base, &input.mask_id)?;
    check(cancel)?;
    let mut selection = plane::Plane::from_rgb(&rgb, width, height)?;
    let second = if input.action.combines() {
        let id = input
            .other_mask_id
            .as_deref()
            .ok_or_else(|| invalid("Choose a second mask."))?;
        let (rgb, width, height) = render.local_mask_coverage(&photo, base, id)?;
        check(cancel)?;
        Some(plane::Plane::from_rgb(&rgb, width, height)?)
    } else {
        None
    };
    if photographic::handles(input.action) {
        photographic::edit(state, input, photo, &mut selection, cancel)?;
    } else if advanced::handles(input.action) {
        advanced::edit(state, input, photo, &mut selection, second.as_ref(), cancel)?;
    } else if input.action == Action::TransformSelection {
        selection.transform(
            input
                .transform
                .ok_or_else(|| invalid("Enter a selection transform."))?,
            cancel,
        )?;
    } else if input.action == Action::KeepConnected {
        selection.keep_connected(
            input.point.ok_or_else(|| invalid("Choose a seed point."))?,
            cancel,
        )?;
    } else {
        selection.edit(input.action, input.value, second.as_ref(), cancel)?;
    }
    Ok(selection)
}

fn save(
    state: &AppState,
    input: &Input,
    ids: (ProjectId, PhotoId),
    base: &Recipe,
    proposal: &Recipe,
    cancel: &CancelToken,
) -> AuraResult<Recipe> {
    let (project, photo) = ids;
    // Stored extension numbers use canonical precision. Re-encoding a matte may spell
    // 0 as 0.0 in its temporary JSON without changing the authored recipe at all.
    if aura_recipe::hash::recipe_hash(base)? == aura_recipe::hash::recipe_hash(proposal)? {
        check(cancel)?;
        return Ok(base.clone());
    }
    let (mut merged, mut changes) = schema::merge(base, proposal, EditSource::User)?;
    // Generic leaf merging cannot delete obsolete dynamic matte IDs. The explicit
    // selection edit owns just these two blocks, retaining every other manual field.
    for key in [
        local_masks::KEY,
        local_masks::MATTE_KEY,
        aura_recipe::studio_color::KEY,
    ] {
        if merged.extra.get(key) != proposal.extra.get(key) {
            if let Some(value) = proposal.extra.get(key) {
                merged.extra.insert(key.into(), value.clone());
            } else {
                merged.extra.remove(key);
            }
            if !changes.changed.iter().any(|path| path == key) {
                changes.changed.push(key.into());
            }
            if !merged
                .provenance
                .user_edited_fields
                .iter()
                .any(|path| path == key)
            {
                merged.provenance.user_edited_fields.push(key.into());
            }
        }
    }
    schema::Validation::check(&merged)?;
    check(cancel)?;
    if !changes.changed.is_empty() {
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &merged,
            &changes.changed,
            input.action.label(),
            &input.recipe_hash,
        )?;
    }
    Ok(merged)
}
