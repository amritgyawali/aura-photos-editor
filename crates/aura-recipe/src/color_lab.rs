//! Original selected colour and tone methods, carried outside frozen recipe contracts.
use crate::{errors::recipe_invalid, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const KEY: &str = "color_lab_v1";
pub const MAX_OPERATIONS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    SpatialNeutralBridge,
    TonalNeutralBridge,
    ChromaReject,
    PaletteAxis,
    DecorrelationStretch,
    PrincipalChromaSuppress,
    SoftGamut,
    LumaChromaCompress,
    ChromaCusp,
    NeutralExclusion,
    HistogramEqualize,
    ClippedEqualize,
    LogEqualize,
    RankGaussian,
    BimodalSeparate,
    PercentileRemap,
    PhotographicMap,
    AsinhMap,
    LogToeShoulder,
    LocalLogContrast,
    RetinexRatio,
    ChromaEdgeProtect,
    ChromaWinsorize,
    OpponentContrast,
    SampledReplace,
    MedianAnchor,
    InterquartileContrast,
    LowerTailLift,
    UpperTailShoulder,
    LuminanceSkewCorrect,
    RmsExposure,
    HarmonicExposure,
    EntropyZoneContrast,
    MinkowskiIlluminant,
    GrayEdgeIlluminant,
    TrimmedIlluminant,
    LogRatioIlluminant,
    ChromaModeNeutral,
    TonalCastEqualize,
    MahalanobisChromaClip,
    ChromaRadiusEqualize,
    HueGeodesicPull,
    PaletteReflection,
    ColorfulnessNormalize,
    HighlightHueRestore,
    LocalChromaCoherence,
    LocalIlluminationFlatten,
    LocalLogRatioDenoise,
    ChromaHueDiffusion,
    HueSectorCompress,
    SigmoidContrast,
    ExponentialShoulder,
    HermiteTone,
    SchlickBias,
    SolarizeLuma,
    ZoneExposure,
    BlackFloor,
    DarkChannelDehaze,
    LocalWienerTone,
    LocalMidrange,
    ChromaShear,
    ChromaSpiral,
    TonalHueTwist,
    DuotoneGradient,
    ChannelCrossfeed,
    SelectiveDensity,
    BradfordAdaptation,
    ChannelRatioLimit,
    MedianDespill,
    HarmonicHueSnap,
    ChromaLumaDetrend,
    GuidedChroma,
    MedianChroma,
    ChannelRankAlign,
    BilateralChroma,
    SpatialChromaPlane,
    RadialLogIllumination,
    GaussianPaletteTransport,
    PaletteSimplex,
    HueDensityEqualize,
    ChromaCopulaMap,
    HueRadiusBalance,
    KmeansPaletteRecolor,
    ChannelRegressionRecover,
    KubelkaMunkMix,
    PcaMonochrome,
    FisherMonochrome,
    IsoluminantEdgeLight,
    LuminanceErrorDiffusion,
    AmplitudeDotscreen,
    IsoLumaContours,
    ExposureFusion,
    PerceptualLightnessLift,
    UvCastTranslate,
    ChromaGroupLasso,
    GeometricMedianNeutral,
    AitchisonTint,
    EnergyConservingLightness,
    DichromaticSpecular,
    MirrorHueFold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub tool: Tool,
    pub mask_id: String,
    pub enabled: bool,
    /// Final blend fraction. Zero is bit-exact identity.
    pub amount: f32,
    /// Method intensity, distinct from final blending.
    pub strength: f32,
    /// Local radius as a fraction of the shorter image edge.
    pub radius: f32,
    /// Luminance anchor in linear working-space units.
    pub pivot: f32,
    /// Chroma distance or compression knee, depending on the named method.
    pub width: f32,
    /// Explicit full-frame reference locations, sampled before this operation.
    pub source: [f32; 2],
    pub second: [f32; 2],
    /// Target in linear Rec.2020; never a display-encoded colour.
    pub target: [f32; 3],
}

fn bounded(v: f32, lo: f32, hi: f32) -> bool {
    v.is_finite() && (lo..=hi).contains(&v)
}
/// # Errors
/// Rejects oversized stacks, ambiguous IDs and all invalid controls.
pub fn validate(operations: &[Operation]) -> AuraResult<()> {
    let mut ids = BTreeSet::new();
    if operations.len() > MAX_OPERATIONS
        || operations.iter().any(|op| {
            op.id.is_empty()
                || op.id.len() > 100
                || !ids.insert(&op.id)
                || op.mask_id.is_empty()
                || op.mask_id.len() > 100
                || !bounded(op.amount, 0.0, 1.0)
                || !bounded(op.strength, 0.0, 1.0)
                || !bounded(op.radius, 0.001, 0.25)
                || !bounded(op.pivot, 0.001, 4.0)
                || !bounded(op.width, 0.001, 2.0)
                || !op
                    .source
                    .iter()
                    .chain(&op.second)
                    .all(|v| bounded(*v, 0.0, 1.0))
                || !op.target.iter().all(|v| bounded(*v, 0.0, 4.0))
        })
    {
        return Err(recipe_invalid(
            KEY,
            "Invalid selected colour method, reference, or controls",
        ));
    }
    Ok(())
}
/// # Errors
/// Rejects malformed stored controls; absent extensions are neutral.
pub fn read(recipe: &Recipe) -> AuraResult<Vec<Operation>> {
    let operations = recipe.extra.get(KEY).map_or_else(
        || Ok(Vec::new()),
        |v| {
            serde_json::from_value(v.clone())
                .map_err(|_| recipe_invalid(KEY, "Invalid colour lab stack"))
        },
    )?;
    validate(&operations)?;
    Ok(operations)
}
/// # Errors
/// Refuses invalid controls without changing unrelated fields.
pub fn write(recipe: &mut Recipe, operations: &[Operation]) -> AuraResult<()> {
    validate(operations)?;
    recipe.extra.insert(
        KEY.into(),
        serde_json::to_value(operations)
            .map_err(|_| recipe_invalid(KEY, "Cannot encode colour lab stack"))?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn operation() -> Operation {
        Operation {
            id: "one".into(),
            tool: Tool::SoftGamut,
            mask_id: "mask".into(),
            enabled: true,
            amount: 0.5,
            strength: 0.5,
            radius: 0.02,
            pivot: 0.18,
            width: 0.2,
            source: [0.25, 0.5],
            second: [0.75, 0.5],
            target: [0.3, 0.2, 0.1],
        }
    }
    #[test]
    fn validates_all_reference_domains_and_duplicate_identity() {
        assert!(validate(&[operation()]).is_ok());
        assert!(validate(&[operation(), operation()]).is_err());
        for field in 0..6 {
            let mut op = operation();
            match field {
                0 => op.amount = f32::NAN,
                1 => op.source[0] = -0.01,
                2 => op.second[1] = 1.1,
                3 => op.target[0] = f32::INFINITY,
                4 => op.radius = 0.5,
                _ => op.width = 0.0,
            }
            assert!(validate(&[op]).is_err());
        }
        assert!(validate(&vec![operation(); MAX_OPERATIONS + 1]).is_err());
    }
    #[test]
    fn old_recipe_neutral_roundtrip_and_invalid_write_atomic() {
        let mut recipe = crate::fixtures::reference();
        assert!(read(&recipe).unwrap().is_empty());
        let before = recipe.clone();
        write(&mut recipe, &[operation()]).unwrap();
        assert_eq!(read(&recipe).unwrap(), vec![operation()]);
        let valid = recipe.clone();
        let mut bad = operation();
        bad.pivot = 0.0;
        assert!(write(&mut recipe, &[bad]).is_err());
        assert_eq!(recipe, valid);
        recipe.extra.remove(KEY);
        assert_eq!(recipe, before);
    }
    #[test]
    fn new_methods_roundtrip_and_reject_nonfinite_controls_atomically() {
        let tools = [
            Tool::SigmoidContrast,
            Tool::ExponentialShoulder,
            Tool::HermiteTone,
            Tool::SchlickBias,
            Tool::SolarizeLuma,
            Tool::ZoneExposure,
            Tool::BlackFloor,
            Tool::DarkChannelDehaze,
            Tool::LocalWienerTone,
            Tool::LocalMidrange,
            Tool::ChromaShear,
            Tool::ChromaSpiral,
            Tool::TonalHueTwist,
            Tool::DuotoneGradient,
            Tool::ChannelCrossfeed,
            Tool::SelectiveDensity,
            Tool::BradfordAdaptation,
            Tool::ChannelRatioLimit,
            Tool::MedianDespill,
            Tool::HarmonicHueSnap,
            Tool::ChromaLumaDetrend,
            Tool::GuidedChroma,
            Tool::MedianChroma,
            Tool::ChannelRankAlign,
            Tool::BilateralChroma,
            Tool::SpatialChromaPlane,
            Tool::RadialLogIllumination,
            Tool::GaussianPaletteTransport,
            Tool::PaletteSimplex,
            Tool::HueDensityEqualize,
            Tool::ChromaCopulaMap,
            Tool::HueRadiusBalance,
            Tool::KmeansPaletteRecolor,
            Tool::ChannelRegressionRecover,
            Tool::KubelkaMunkMix,
            Tool::PcaMonochrome,
            Tool::FisherMonochrome,
            Tool::IsoluminantEdgeLight,
            Tool::LuminanceErrorDiffusion,
            Tool::AmplitudeDotscreen,
            Tool::IsoLumaContours,
            Tool::ExposureFusion,
            Tool::PerceptualLightnessLift,
            Tool::UvCastTranslate,
            Tool::ChromaGroupLasso,
            Tool::GeometricMedianNeutral,
            Tool::AitchisonTint,
            Tool::EnergyConservingLightness,
            Tool::DichromaticSpecular,
            Tool::MirrorHueFold,
        ];
        for tool in tools {
            let mut op = operation();
            op.tool = tool;
            let mut recipe = crate::fixtures::reference();
            write(&mut recipe, &[op.clone()]).unwrap();
            assert_eq!(read(&recipe).unwrap(), vec![op.clone()]);
            let before = recipe.clone();
            for field in 0..9 {
                let mut bad = op.clone();
                match field {
                    0 => bad.amount = f32::INFINITY,
                    1 => bad.strength = f32::NAN,
                    2 => bad.radius = f32::NEG_INFINITY,
                    3 => bad.pivot = f32::NAN,
                    4 => bad.width = f32::INFINITY,
                    5 => bad.source[0] = f32::NAN,
                    6 => bad.second[1] = f32::INFINITY,
                    7 => bad.target[0] = f32::NAN,
                    _ => bad.target[2] = -0.1,
                }
                assert!(
                    write(&mut recipe, &[bad]).is_err(),
                    "{tool:?}, field {field}"
                );
                assert_eq!(recipe, before);
            }
        }
    }
}
