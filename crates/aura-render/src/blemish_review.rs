//! Read-only remaining-mark review inside an explicit retouch selection.
//! These are measured candidate areas, never a diagnosis or a completeness guarantee.
#![allow(clippy::indexing_slicing)]
use aura_recipe::retouch_tools::{Edit, Matte, Tool};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub region: [f32; 4],
    pub pixels: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub regions: Vec<Region>,
    pub measured_areas: usize,
    pub selected_mark_pixels: usize,
}

/// Measure compact residual marks on the current linear working-space frame.
/// No pixels, stored selection, recipe or history are modified.
#[must_use]
pub fn probe(selected: &Edit) -> Edit {
    let mut probe = selected.clone();
    probe.tool = Tool::AcneClear;
    probe.source = None;
    probe.source_scale = 1.0;
    probe.target_color = None;
    probe.repair_passes = Some(1);
    probe.texture_heal = false;
    probe.clean_ring_fit = false;
    probe.curved_heal = false;
    probe.heal_samples.clear();
    probe.texture_sources.clear();
    probe.amount = 1.0;
    probe.tone = 1.0;
    probe.enabled = true;
    probe
}

/// Inspect this explicit selection without modifying the supplied working-space pixels.
#[must_use]
pub fn measure(
    rgb: &[f32],
    w: usize,
    h: usize,
    selected: &Edit,
    mattes: &BTreeMap<String, Matte>,
) -> Review {
    let probe = probe(selected);
    let mut plane = crate::retouch_tools::frequency_heal_marks(rgb, w, h, &probe, mattes);
    let mut regions = Vec::new();
    let mut measured_areas = 0;
    let mut selected_mark_pixels = 0;
    // Core coverage rather than the feather around the mark. The detector has already
    // refused long structures and respected the selection's feature exclusions.
    for start in 0..plane.len() {
        if plane[start] < 0.35 {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        plane[start] = 0.0;
        let (mut x0, mut y0, mut x1, mut y1, mut count) = (w, h, 0, 0, 0);
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            count += 1;
            for next in [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then_some(i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then_some(i + w),
            ]
            .into_iter()
            .flatten()
            {
                if plane[next] >= 0.35 {
                    plane[next] = 0.0;
                    queue.push_back(next);
                }
            }
        }
        if count < 3 {
            continue;
        }
        measured_areas += 1;
        selected_mark_pixels += count;
        if regions.len() < 256 {
            regions.push(Region {
                region: [
                    (x0 + x1 + 1) as f32 / (2 * w) as f32,
                    (y0 + y1 + 1) as f32 / (2 * h) as f32,
                    ((x1 - x0 + 3) as f32 / (2 * w) as f32).max(0.001),
                    ((y1 - y0 + 3) as f32 / (2 * h) as f32).max(0.001),
                ],
                pixels: count,
            });
        }
    }
    regions.sort_by_key(|region| std::cmp::Reverse(region.pixels));
    Review {
        regions,
        measured_areas,
        selected_mark_pixels,
    }
}
