//! Explicit spatial algorithms; source coordinates and parameters are bounded.
use crate::{errors::recipe_invalid, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const KEY: &str = "spatial_lab_v1";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    AnisotropicDiffusion,
    TotalVariation,
    NonLocalMeans,
    VectorMedoid,
    TrimmedMean,
    AdaptiveWiener,
    HaarDenoise,
    HaarDetail,
    LaplacianDetail,
    Kuwahara,
    TensorFlow,
    BlockDeblock,
    ApertureBokeh,
    Starburst,
    RotationBlur,
    TrajectoryBlur,
    HarmonicFill,
    PoissonDonor,
    ChannelRegister,
    MorphologyClean,
    LocalRankContrast,
    MosquitoSuppress,
    EdgeDirectedSmooth,
    DonorBilateral,
    TextureQuilt,
    GuidedLinear,
    DomainTransform,
    RichardsonLucy,
    ShockSharpen,
    CurvatureFlow,
    OpeningReconstruct,
    ClosingReconstruct,
    ToggleMapping,
    ContraharmonicMean,
    GeometricMean,
    HuberLocation,
    TukeyLocation,
    LocalPlaneResidual,
    ChromaGuidedRegression,
    AnscombeDenoise,
    SideWindowMean,
    MedianOfMeans,
    BilateralSharpen,
    AiryDiffraction,
    AnnularBokeh,
    TiltShiftIntegration,
    ZoomIntegration,
    RadialChromatic,
    PerspectivePlane,
    ThinPlateWarp,
    AdaptiveMedianImpulse,
    SigmaRangeMean,
    SpatialWeightedMedian,
    LuminanceWinsor,
    ChromaBilateral,
    ChromaCovariance,
    RgbCovariance,
    EntropyDetail,
    RmsContrast,
    LogRetinex,
    PeriodicNotch,
    CrossMedian,
    HessianRidge,
    CoherenceDetail,
    DirectionalMedian,
    QuadraticResidual,
    LanczosShift,
    EllipticalAperture,
    HexagonalAperture,
    CatEyeAperture,
    HighlightStreak,
    BilateralToneCompress,
    DarkChannelHaze,
    ConnectedImpulse,
    ChromaMedian,
    LocalDctShrink,
    Cdf53Shrink,
    GeometricMedian,
    MeanShiftColour,
    HodgesLehmann,
    GeodesicRange,
    PatchLowRank,
    MahalanobisRange,
    LeeSpeckle,
    FrostSpeckle,
    GammaMapSpeckle,
    BiharmonicStencil,
    FractionalLaplacian,
    ParabolicBackground,
    DirectionalScratch,
    PhaseOnlyDetail,
    SpectralResidualDetail,
    HampelImpulse,
    CircularHueSmooth,
    ChromaMagnitudeMedian,
    WhitePatchAdapt,
    GrayEdgeAdapt,
    LocalSkewnessBalance,
    LocalKurtosisDetail,
    MinmodEdgeLimiter,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub mask_id: String,
    pub tool: Tool,
    pub enabled: bool,
    pub amount: f32,
    pub radius: f32,
    pub threshold: f32,
    pub detail: f32,
    pub angle: f32,
    pub source: [f32; 2],
    pub shift: [f32; 2],
    /// Explicit normalized trajectory offsets, used only by trajectory blur.
    pub path: Vec<[f32; 2]>,
}
/// # Errors
/// Refuses duplicate IDs, nonfinite/oversized parameters and unbounded spatial work.
pub fn validate(ops: &[Operation]) -> AuraResult<()> {
    let bound = |v: f32, lo: f32, hi: f32| v.is_finite() && (lo..=hi).contains(&v);
    let mut ids = BTreeSet::new();
    if ops.len() > 32
        || ops.iter().any(|op| {
            op.id.is_empty()
                || op.id.len() > 100
                || !ids.insert(&op.id)
                || op.mask_id.is_empty()
                || op.mask_id.len() > 100
                || !bound(op.amount, 0.0, 1.0)
                || !bound(op.radius, 0.002, 0.15)
                || !bound(op.threshold, 0.0001, 1.0)
                || !bound(op.detail, -1.0, 2.0)
                || !bound(op.angle, -180.0, 180.0)
                || op.source.iter().any(|v| !bound(*v, 0.0, 1.0))
                || op.shift.iter().any(|v| !bound(*v, -0.1, 0.1))
                || !(2..=16).contains(&op.path.len())
                || op.path.iter().flatten().any(|v| !bound(*v, -0.2, 0.2))
        })
    {
        return Err(recipe_invalid(
            KEY,
            "Invalid spatial tool, selection or parameters",
        ));
    }
    Ok(())
}
/// # Errors
/// Refuses invalid stored spatial operations.
pub fn read(recipe: &Recipe) -> AuraResult<Vec<Operation>> {
    let ops = recipe.extra.get(KEY).map_or_else(
        || Ok(Vec::new()),
        |v| {
            serde_json::from_value(v.clone())
                .map_err(|_| recipe_invalid(KEY, "Invalid spatial stack"))
        },
    )?;
    validate(&ops)?;
    Ok(ops)
}
/// # Errors
/// Refuses malformed operations without altering other recipe fields.
pub fn write(recipe: &mut Recipe, ops: &[Operation]) -> AuraResult<()> {
    validate(ops)?;
    let value = serde_json::to_value(ops)
        .map_err(|_| recipe_invalid(KEY, "Cannot encode spatial tools"))?;
    recipe.extra.insert(KEY.into(), value);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn op(tool: Tool) -> Operation {
        Operation {
            id: "one".into(),
            mask_id: "selected".into(),
            tool,
            enabled: true,
            amount: 0.7,
            radius: 0.04,
            threshold: 0.05,
            detail: 0.7,
            angle: 25.0,
            source: [0.2, 0.2],
            shift: [0.02, -0.01],
            path: vec![[-0.04, 0.0], [0.0, 0.03], [0.04, 0.0]],
        }
    }
    #[test]
    fn malformed_work_is_refused() {
        let original = op(Tool::HaarDenoise);
        assert!(validate(std::slice::from_ref(&original)).is_ok());
        assert!(validate(&[original.clone(), original.clone()]).is_err());
        let mut invalid = original;
        invalid.threshold = f32::NAN;
        assert!(validate(&[invalid]).is_err());
        let mut invalid = op(Tool::HaarDenoise);
        invalid.path = vec![[0.0, 0.0]; 17];
        assert!(validate(&[invalid]).is_err());
    }
    #[test]
    fn legacy_recipes_are_neutral_and_round_trip() {
        let mut recipe = crate::fixtures::reference();
        assert!(read(&recipe).unwrap().is_empty());
        let ops = vec![op(Tool::HaarDenoise)];
        write(&mut recipe, &ops).unwrap();
        assert_eq!(read(&recipe).unwrap(), ops);
    }
}
