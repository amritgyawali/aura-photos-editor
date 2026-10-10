//! One bounded residual pass after rendering protected first-pass repairs.
use super::{after_edits, plan, FeatureEdits, Matte, Pixels, PortraitFace, Settings, Tool};
use aura_core::AuraResult;
use aura_recipe::retouch_tools::{Edit, Matte as RecipeMatte};
use std::collections::BTreeMap;

fn inside_repaired_region(point: [f32; 2], edit: &Edit) -> bool {
    ((point[0] - edit.region[0]) / edit.region[2])
        .hypot((point[1] - edit.region[1]) / edit.region[3])
        < 1.0
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn refine(
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
    let available = usize::from(settings.max_spots)
        .min(900)
        .saturating_sub(out.blemishes.len());
    if out.blemishes.is_empty() || available == 0 {
        return Ok(());
    }
    // Guard copies only. The actual edits receive their single final guard later.
    let mut guarded = out.blemishes.clone();
    let mut preview_mattes = mattes.clone();
    super::super::eye_guard::protect(
        face,
        original,
        guarded.iter_mut(),
        &mut preview_mattes,
        &format!("{prefix}{index}-feature-guard"),
        settings,
    )?;
    let Some(bytes) = after_edits(working, &guarded, &preview_mattes) else {
        out.report
            .findings
            .push("Residual skin check unavailable; first-pass repairs were kept.".into());
        return Ok(());
    };
    let Some(pixels) = Pixels::new(&bytes, working.width as u32, working.height as u32) else {
        out.report
            .findings
            .push("Residual skin frame invalid; first-pass repairs were kept.".into());
        return Ok(());
    };
    let next_settings = Settings {
        max_spots: u16::try_from(available).unwrap_or(900),
        ..*settings
    };
    let next = plan(face, index, &pixels, prefix, &next_settings, matte);
    let mut additional = Vec::new();
    for mut edit in next.blemishes {
        // Keep the complete previous footprint, including its blend, outside
        // residual detection. Otherwise reconstructed pores or a blend can be
        // mistaken for new inflammation and create a visible repair ring.
        if (!edit.clean_ring_fit && edit.tool != Tool::SkinUniformity)
            || out
                .blemishes
                .iter()
                .any(|old| inside_repaired_region([edit.region[0], edit.region[1]], old))
        {
            continue;
        }
        edit.id = format!(
            "{prefix}{index}-spot-deep-{}",
            out.blemishes.len() + additional.len()
        );
        additional.push(edit);
    }
    out.report.findings.push(format!("Residual skin check: {} additional inflamed spot(s) repaired after the protected first pass; no new repairs centered inside previous repair footprints.",additional.len()));
    out.blemishes.extend(additional);
    out.report.spots_healed = out.blemishes.len();
    Ok(())
}
