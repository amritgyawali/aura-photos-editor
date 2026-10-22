//! Selection laboratory request validation and routing. Existing atomic history is reused.
use super::{check, invalid, plane::Plane, Action, Input};
use crate::AppState;
use aura_core::{progress::CancelToken, AuraResult, PhotoId};
use aura_recipe::{
    local_masks::{self, Component, LocalMask, Mode, Source},
    retouch_tools::Matte,
    Recipe,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub vertices: Vec<[f32; 2]>,
    pub bounds: [f32; 4],
    pub range: [f32; 2],
    pub tolerance: f32,
    pub radius: u32,
    pub corner: f32,
    pub angle: f32,
    pub sweep: f32,
    pub seed_index: usize,
    pub min_area: usize,
    pub max_area: usize,
    pub compactness: f32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            vertices: vec![[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]],
            bounds: [0.2, 0.2, 0.8, 0.8],
            range: [0.2, 0.8],
            tolerance: 0.15,
            radius: 4,
            corner: 0.1,
            angle: 0.0,
            sweep: 90.0,
            seed_index: 0,
            min_area: 4,
            max_area: 262_144,
            compactness: 0.0,
        }
    }
}
impl Options {
    pub(super) fn validated(input: &Input) -> AuraResult<Self> {
        let options = input.advanced.clone().unwrap_or_default();
        let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        if options.vertices.len() > 128
            || options.vertices.iter().flatten().any(|v| !unit(*v))
            || options.bounds.iter().any(|v| !unit(*v))
            || options.bounds[0] >= options.bounds[2]
            || options.bounds[1] >= options.bounds[3]
            || options.range.iter().any(|v| !unit(*v))
            || options.range[0] > options.range[1]
            || !unit(options.tolerance)
            || options.tolerance < 0.001
            || options.radius == 0
            || options.radius > 32
            || !unit(options.corner)
            || !options.angle.is_finite()
            || !(-360.0..=360.0).contains(&options.angle)
            || !options.sweep.is_finite()
            || !(0.1..=360.0).contains(&options.sweep)
            || options.min_area > options.max_area
            || options.max_area > 262_144
            || !unit(options.compactness)
        {
            return Err(invalid("Enter finite normalized coordinates, ordered bounds/ranges, radius 1-32 and valid region limits."));
        }
        if input.point.is_some_and(|p| p.iter().any(|v| !unit(*v))) {
            return Err(invalid("Sample coordinates must lie inside the photo."));
        }
        if matches!(input.action, Action::TextureRange | Action::GuidedSmooth) && options.radius > 8
        {
            return Err(invalid(
                "Texture and guided smoothing are limited to eight selection cells.",
            ));
        }
        Ok(options)
    }
}
pub(super) fn manages(action: Action) -> bool {
    matches!(
        action,
        Action::SplitRegions
            | Action::AppendStack
            | Action::ReplaceSelection
            | Action::SyncAdjustments
    )
}
pub(super) fn handles(action: Action) -> bool {
    matches!(
        action,
        Action::PolygonMarquee
            | Action::RoundedMarquee
            | Action::SectorMarquee
            | Action::SampleColour
            | Action::ColourWand
            | Action::SampleLuminance
            | Action::SaturationRange
            | Action::TextureRange
            | Action::ImageEdges
            | Action::GuidedSmooth
            | Action::EdgeGrow
            | Action::InteriorFalloff
            | Action::NegativeSpace
            | Action::Skeleton
            | Action::AlignCentroid
            | Action::FitBounds
            | Action::PerspectiveSelection
            | Action::AutoThreshold
            | Action::ChromaKey
            | Action::VoronoiPartition
            | Action::FilterRegions
    )
}
pub(super) fn store_plane(
    plane: &Plane,
    mattes: &mut BTreeMap<String, Matte>,
) -> AuraResult<Vec<Component>> {
    let matte = plane.matte();
    let bytes = serde_json::to_vec(&matte).map_err(|_| invalid("Cannot encode selection."))?;
    let id = format!("selection-{}", blake3::hash(&bytes).to_hex());
    mattes.insert(id.clone(), matte);
    Ok(vec![Component {
        mode: Mode::Add,
        invert: false,
        source: Source::Matte {
            matte: id,
            what: "selection laboratory".into(),
        },
    }])
}
pub(super) fn manage(
    state: &AppState,
    input: &Input,
    photo: PhotoId,
    base: &Recipe,
    masks: &mut Vec<LocalMask>,
    mattes: &mut BTreeMap<String, Matte>,
    cancel: &CancelToken,
) -> AuraResult<String> {
    let options = Options::validated(input)?;
    let source = masks
        .iter()
        .find(|m| m.id == input.mask_id)
        .cloned()
        .ok_or_else(|| invalid("Missing source mask."))?;
    if input.action == Action::SplitRegions {
        let (rgb, w, h) = state
            .render()?
            .local_mask_coverage(&photo, base, &input.mask_id)?;
        let plane = Plane::from_rgb(&rgb, w, h)?;
        let groups = plane.region_groups(cancel)?;
        let groups: Vec<_> = groups
            .into_iter()
            .filter(|g| g.len() >= options.min_area && g.len() <= options.max_area)
            .collect();
        if groups.len() < 2 {
            return Err(invalid("At least two disconnected regions must meet the minimum area to split this selection."));
        }
        if masks.len() + groups.len() > local_masks::MAX_MASKS {
            return Err(invalid(
                "Splitting exceeds the 32-mask limit. Raise the minimum region area.",
            ));
        }
        let mut first = None;
        for (index, group) in groups.iter().enumerate() {
            check(cancel)?;
            let region = plane.isolate(group);
            let mut mask = source.clone();
            mask.id = format!("mask-{}", uuid::Uuid::new_v4().simple());
            mask.name = format!(
                "{} region {}",
                source.name.chars().take(16).collect::<String>(),
                index + 1
            );
            mask.components = store_plane(&region, mattes)?;
            if first.is_none() {
                first = Some(mask.id.clone());
            }
            masks.push(mask);
        }
        return first.ok_or_else(|| invalid("No selection regions found."));
    }
    let target = masks
        .iter_mut()
        .find(|m| Some(&m.id) == input.other_mask_id.as_ref())
        .ok_or_else(|| invalid("Choose a destination mask."))?;
    match input.action {
        Action::AppendStack => {
            if target.components.len() + source.components.len() > local_masks::MAX_COMPONENTS {
                return Err(invalid("Appending exceeds the component limit."));
            }
            target.components.extend(source.components);
        }
        Action::ReplaceSelection => target.components = source.components,
        Action::SyncAdjustments => {
            target.params = source.params;
            target.amount = source.amount;
        }
        _ => return Err(invalid("Unknown mask management action.")),
    }
    Ok(target.id.clone())
}
pub(super) fn edit(
    state: &AppState,
    input: &Input,
    photo: PhotoId,
    plane: &mut Plane,
    other: Option<&Plane>,
    cancel: &CancelToken,
) -> AuraResult<()> {
    let options = Options::validated(input)?;
    let point = input.point.unwrap_or([0.5, 0.5]);
    match input.action {
        Action::PolygonMarquee
        | Action::RoundedMarquee
        | Action::SectorMarquee
        | Action::FitBounds
        | Action::PerspectiveSelection
        | Action::AlignCentroid => plane.geometry(input.action, &options, other, cancel),
        Action::InteriorFalloff
        | Action::NegativeSpace
        | Action::Skeleton
        | Action::AutoThreshold
        | Action::VoronoiPartition
        | Action::FilterRegions => plane.topology(input.action, &options, cancel),
        _ => {
            let frame = super::advanced_pixels::Frame::load(
                state,
                &input.project_id,
                photo,
                plane.width,
                plane.height,
            )?;
            plane.pixel_selection(input.action, &options, point, &frame, cancel)
        }
    }
}
