//! The edit a reference received, learned from its original (ADR-0160).
//!
//! A finished photograph of another scene cannot say which of its colours are its edit and which
//! are its scene: a beige wall and a warm grade measure the same. Its original can. The difference
//! between the two is the edit, and the same edit applied to every photograph of a batch is what a
//! photographer means by "match this". Measured on one real wedding shoot under eight known
//! looks, applying the learned edit left 1.94 of mean `OKLab` difference from the right answer
//! where matching appearance alone left 4.16 and doing nothing 4.74.
//!
//! The original is the photograph being matched when it is structurally the same frame as the
//! reference, or a photograph in the same collection whose file name the reference's name
//! contains and whose structure confirms it. Nothing here guesses beyond those two.
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_recipe::reference_grade::{self as grade_model, ReferenceGrade};
use serde::{Deserialize, Serialize};

use super::{content, fitting_frame, rendered, transfer, AppState, Reference};

const EDIT: &str = "colour_reference_edit_v1:";
const VERSION: u32 = 1;
/// Structural similarity at or above which a photograph is the reference's own frame. The
/// signed luminance-gradient correlation `content::similarity` measures is 0.99 and above for
/// the same frame under any colour edit and below 0.6 for two frames of one burst.
pub(super) const SAME_FRAME: f32 = 0.9;
/// A learned edit that cannot reproduce its own reference within this distance was learned
/// from something that only resembles the original, and is not kept.
const MAX_RESIDUAL: f32 = 3.0;
/// At most this many photographs named like the reference are rendered to confirm one.
const MAX_CANDIDATES: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LearnedEdit {
    pub version: u32,
    pub grade: ReferenceGrade,
    pub photo_id: String,
    pub source: String,
    pub similarity: f32,
    pub residual: f32,
}

fn key(reference_id: &str) -> AuraResult<String> {
    super::asset_key(reference_id)?;
    Ok(format!("{EDIT}{reference_id}"))
}

pub(super) fn recall(state: &AppState, reference_id: &str) -> AuraResult<Option<LearnedEdit>> {
    Ok(super::read::<LearnedEdit>(state, &key(reference_id)?)
        .ok()
        .flatten()
        .filter(|e| e.version == VERSION && e.grade.validate().is_ok()))
}

/// Keep `edit` unless a closer original is already known.
pub(super) fn remember(state: &AppState, reference_id: &str, edit: &LearnedEdit) -> AuraResult<()> {
    if edit.residual > MAX_RESIDUAL {
        return Ok(());
    }
    if let Some(known) = recall(state, reference_id)? {
        if known.photo_id != edit.photo_id && known.similarity >= edit.similarity {
            return Ok(());
        }
    }
    super::store(state, &key(reference_id)?, edit)
}

/// The reference's stored thumbnail, decoded.
fn reference_thumbnail(reference: &Reference) -> Option<aura_raw::codec::Rgb8> {
    let text = reference.summary.thumbnail.as_deref()?;
    let encoded = text.strip_prefix("data:image/png;base64,")?;
    let bytes = base64_decode(encoded);
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .ok()?
        .to_rgb8();
    Some(aura_raw::codec::Rgb8 {
        width: image.width(),
        height: image.height(),
        data: image.into_raw(),
    })
}

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for v in text.bytes().filter_map(value) {
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// Learn the edit from a photograph already rendered without any reference grade: fitted from
/// the reference's statistics, then refined on corresponding pixels of the two thumbnails,
/// which line up because this is the reference's own frame.
pub(super) fn learn(
    stats: &transfer::GradeStats,
    reference: &Reference,
    plain: &aura_raw::codec::Rgb8,
    similarity: f32,
    photo_id: &str,
    source: &str,
    mut checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<LearnedEdit> {
    let pixels = transfer::samples(plain, transfer::TARGET_SAMPLES);
    let mut fit = transfer::fit_with_checkpoint(stats, &pixels, Some(similarity), &mut checkpoint)?;
    if let Some(pairs) = transfer::thumbnail(plain)
        .zip(reference_thumbnail(reference))
        .and_then(|(photo, theirs)| transfer::Pairs::new(&photo, &theirs))
    {
        let (refined, _) = transfer::refine_on_pairs(&fit.grade, &pairs, &mut checkpoint)?;
        fit.after = transfer::predicted(stats, &refined, &pixels).1;
        fit.grade = refined;
    }
    Ok(LearnedEdit {
        version: VERSION,
        grade: fit.grade,
        photo_id: photo_id.to_owned(),
        source: source.to_owned(),
        similarity,
        residual: fit.after.total,
    })
}

/// The photograph's own file name, for the report.
pub(super) fn file_name(state: &AppState, photo: &PhotoId) -> AuraResult<String> {
    let id = photo.to_db();
    state.catalog().read(|conn| {
        conn.query_row(
            "SELECT file_name FROM photo_file WHERE photo_id=?1 AND role='primary' LIMIT 1",
            [&id],
            |row| row.get::<_, String>(0),
        )
        .or_else(|_| Ok(String::new()))
        .map_err(|e: rusqlite::Error| {
            aura_core::errors::db::statement_failed("reference original", &e)
        })
    })
}

fn stem(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    name.rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .to_ascii_lowercase()
}

/// Photographs in the collection whose file name the reference's name contains: the export
/// `989A2698-edit.jpg` names `989A2698.CR3`. Short stems match too much to be evidence.
fn named_like(
    state: &AppState,
    project: &ProjectId,
    reference_name: &str,
    except: &PhotoId,
) -> AuraResult<Vec<(PhotoId, String)>> {
    let wanted = stem(reference_name);
    let project = project.to_db();
    let rows: Vec<(String, String)> = state.catalog().read(|conn| {
        let mut query = conn
            .prepare(
                "SELECT photo_id, file_name FROM photo_file
                 WHERE project_id = ?1 AND role = 'primary' ORDER BY file_name LIMIT 20000",
            )
            .map_err(|e| aura_core::errors::db::statement_failed("find reference original", &e))?;
        let rows = query
            .query_map([&project], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| aura_core::errors::db::statement_failed("find reference original", &e))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| aura_core::errors::db::statement_failed("find reference original", &e))
    })?;
    let mut found: Vec<(usize, PhotoId, String)> = rows
        .into_iter()
        .filter_map(|(id, name)| {
            let candidate = stem(&name);
            (candidate.len() >= 4 && wanted.contains(&candidate))
                .then(|| {
                    PhotoId::from_db(&id)
                        .ok()
                        .map(|p| (candidate.len(), p, name))
                })
                .flatten()
        })
        .filter(|(_, p, _)| p != except)
        .collect();
    // The longest matching name is the most specific.
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
    found.dedup_by(|a, b| a.1 == b.1);
    Ok(found
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, p, n)| (p, n))
        .collect())
}

/// Find the reference's original among the photographs named like it, confirm it by structure,
/// learn the edit from it and remember that. `None` when the collection holds no original.
#[allow(clippy::similar_names)] // state and stats
pub(super) fn search(
    state: &AppState,
    project: &ProjectId,
    reference: &Reference,
    stats: &transfer::GradeStats,
    except: &PhotoId,
    mut checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<Option<LearnedEdit>> {
    for (photo, name) in named_like(state, project, &reference.summary.name, except)? {
        checkpoint()?;
        let mut recipe = crate::develop_commands::load_or_neutral(state, photo)?;
        recipe.extra.remove(grade_model::KEY);
        let plain = rendered(state, &fitting_frame(state, &photo)?, &recipe)?;
        let Some(similarity) = content::similarity(&reference.small_rgb, &plain) else {
            continue;
        };
        if similarity < SAME_FRAME {
            continue;
        }
        let learned = learn(
            stats,
            reference,
            &plain,
            similarity,
            &photo.to_db(),
            &name,
            &mut checkpoint,
        )?;
        if learned.residual <= MAX_RESIDUAL {
            remember(state, &reference.summary.id, &learned)?;
            return Ok(Some(learned));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    #[test]
    fn a_stored_thumbnail_decodes() {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &[10, 20, 30, 40, 50, 60],
                2,
                1,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let text = crate::develop_commands::base64(&bytes);
        assert_eq!(base64_decode(&text), bytes);
    }

    #[test]
    fn an_exported_name_finds_its_camera_file() {
        assert_eq!(stem("C:/out/989A2698-edit.JPG"), "989a2698-edit");
        assert!(stem("989A2698-edit.jpg").contains(&stem("989A2698.CR3")));
        assert!(!stem("final.jpg").contains(&stem("989A2698.CR3")));
    }
}
