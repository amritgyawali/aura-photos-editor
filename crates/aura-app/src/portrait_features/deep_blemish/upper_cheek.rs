//! Add confirmed cheek repairs after the unchanged primary and residual plan.
use super::{
    after_edits, plan_with_cheek, FeatureEdits, Geometry, Matte, Pixels, PortraitFace, Settings,
};
use aura_core::AuraResult;
use aura_recipe::retouch_tools::Matte as RecipeMatte;
use std::collections::BTreeMap;

pub(crate) const ID: &str = "-spot-deep-cheek-";

#[allow(clippy::too_many_arguments)]
pub(crate) fn supplement(
    face: &PortraitFace,
    index: usize,
    original: &Pixels<'_>,
    working: &Pixels<'_>,
    prefix: &str,
    settings: &Settings,
    matte: Option<&Matte>,
    mattes: &BTreeMap<String, RecipeMatte>,
    out: &mut FeatureEdits,
) -> AuraResult<()> {
    let capacity = usize::from(settings.max_spots)
        .min(900)
        .saturating_sub(out.blemishes.len());
    if !settings.deep_blemish_cleanup || !settings.remove_dark_marks || capacity == 0 {
        return Ok(());
    }
    let Some(g) = Geometry::new(face, original) else {
        return Ok(());
    };
    // Keep the established donor/component/residual choices independent of new
    // coverage. Supplement only after their guarded pixels are available.
    let bytes = if out.blemishes.is_empty() {
        None
    } else {
        let mut guarded = out.blemishes.clone();
        let mut masks = mattes.clone();
        super::super::eye_guard::protect(
            face,
            original,
            guarded.iter_mut(),
            &mut masks,
            &format!("{prefix}{index}-feature-guard"),
            settings,
        )?;
        let Some(bytes) = after_edits(working, &guarded, &masks) else {
            out.report
                .findings
                .push("Upper-cheek check unavailable; established repairs were kept.".into());
            return Ok(());
        };
        Some(bytes)
    };
    let observed = bytes
        .as_deref()
        .and_then(|rgb| Pixels::new(rgb, working.width as u32, working.height as u32));
    let observed = observed.as_ref().unwrap_or(working);
    let proposed = plan_with_cheek(face, index, observed, prefix, settings, matte, true);
    let mut added = Vec::new();
    for mut edit in proposed.blemishes {
        let centre = [
            edit.region[0] * original.width as f32,
            edit.region[1] * original.height as f32,
        ];
        if super::super::eye_guard::spot_eye_weight(&g, centre, false) >= 1.0
            || out.blemishes.iter().any(|old| {
                ((edit.region[0] - old.region[0]) / old.region[2])
                    .hypot((edit.region[1] - old.region[1]) / old.region[3])
                    < 1.0
            })
        {
            continue;
        }
        edit.id = format!("{prefix}{index}{ID}{}", added.len());
        added.push(edit);
        if added.len() == capacity {
            break;
        }
    }
    out.report.findings.push(format!("Upper-cheek check: {} supplemental compact repairs proposed after established repairs; remaining operation capacity decides how many can be retained.", added.len()));
    out.blemishes.extend(added);
    out.report.spots_healed = out.blemishes.len();
    Ok(())
}
