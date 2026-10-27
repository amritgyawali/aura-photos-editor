//! Find edited references' own originals in a collection, and learn the edit from the pairs.
//!
//! An edited reference of a different scene cannot be inverted into its edit: object colour,
//! light and grade are mixed in the same pixels, and on real photographs no statistic of it
//! carries across scenes reliably (`transfer/tests/looks.rs`). The reference's original can. When
//! a photographer matches a wedding to its own edited frames, those frames' camera files are
//! usually in the collection - so the edit is learned from corresponding pixels, exactly as the
//! original/edited-pair route does, and the same operation is applied to every photograph.
//!
//! Every reference whose original is found contributes a pair, up to [`MAX_PAIRS`], and the pairs
//! are pooled into one calibration: each shows the edit on its own scene's colours, so together
//! they cover more of what a batch will contain and less of the batch depends on extrapolation.
//!
//! A candidate is a photograph whose file name the reference's name starts with
//! (`989A2698.CR3` for `989A2698-Edit.jpg`), or whose capture time is the reference's to the
//! second. Neither is trusted alone: a pair must correspond in structure, and the calibration
//! refuses held-out pixels a global operation cannot explain - which is what a crop, a retouch or
//! a different frame of a burst produces.
use std::path::{Path, PathBuf};

use aura_core::progress::CancelToken;
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_raw::codec::Rgb8;
use aura_recipe::calibrated_look::CalibratedLook;
use aura_render::{Frame, FrameSource, RenderLevel};

use super::gallery_fit::Allowance;
use super::{
    appearance, calibration, check_cancel, content, gallery_fit, AppState, CalibrationReport,
};

/// Candidates confirmed by pixels for one reference at most; each costs one native decode.
const MAX_CANDIDATES: usize = 6;
/// Pairs pooled into one calibration at most.
pub(crate) const MAX_PAIRS: usize = 8;
/// References looked up at most; a reference folder can hold hundreds.
const MAX_REFERENCES: usize = 200;
/// The longest edge a pair is compared at. A global operation is identified by colours, not
/// by detail, and the edited file is often a smaller export than the camera original.
const EDGE: u32 = 512;
/// The original is averaged down to this before it is rendered, then filtered to the compared
/// size as an export would be. Point-sampling a camera file instead keeps single sensor pixels
/// that an exported photograph averaged away, and every fine texture then reads as an edit the
/// calibration cannot explain.
const RENDER_EDGE: u32 = 1024;

/// Originals found in the collection and the edit learned from them.
#[derive(Debug)]
pub(crate) struct Learned {
    pub photos: Vec<PhotoId>,
    pub names: Vec<String>,
    pub model: CalibratedLook,
    pub report: CalibrationReport,
    /// How far the photographer's edits moved skin and clipping on their originals.
    pub allowance: Allowance,
}

impl Learned {
    /// The originals, as a photographer reads them.
    pub(crate) fn label(&self) -> String {
        match self.names.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            [first, second] => format!("{first} and {second}"),
            [first, rest @ ..] => format!("{first} and {} more", rest.len()),
        }
    }
}

/// What a search of one collection found.
#[derive(Debug, Default)]
pub(crate) struct Search {
    pub learned: Option<Learned>,
    /// Why the best candidate was not used, when one was found and refused - so a photographer
    /// is told the original was seen, rather than left to wonder why the match is an estimate.
    pub refused: Option<String>,
}

/// What the photographer's edit did to the people and the highlights of its own original: the
/// learned look may do as much again, with a quarter more for a target's different light, and
/// never less than the pilot limits.
fn allowance(original: &Rgb8, edited: &Rgb8) -> Allowance {
    let pilot = Allowance::default();
    let Ok(regions) = appearance::analyse(original) else {
        return pilot;
    };
    let before = appearance::readings(original, &regions);
    let after = appearance::readings(edited, &regions);
    let skin = appearance::skin_change(&before, &after);
    let clipping = appearance::clipped(edited) - appearance::clipped(original);
    Allowance {
        skin: if skin.is_finite() {
            pilot.skin.max(1.25 * skin)
        } else {
            pilot.skin
        },
        clipping: pilot.clipping.max(clipping + pilot.clipping),
    }
}

fn stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_lowercase())
        .unwrap_or_default()
}

/// Whether an exported name is the camera file's name with an editor's suffix, or no suffix.
pub(super) fn names_match(reference: &str, photo: &str) -> bool {
    let (reference, photo) = (stem(reference), stem(photo));
    if photo.chars().count() < 3 {
        return false;
    }
    match reference.strip_prefix(photo.as_str()) {
        Some("") => true,
        Some(rest) => rest.starts_with(['-', '_', ' ', '.', '(', '+', '~']),
        None => false,
    }
}

/// One photograph of the collection, as candidates are chosen from it.
struct Shot {
    photo: PhotoId,
    file: String,
    time: Option<time::OffsetDateTime>,
}

fn collection(state: &AppState, project: &ProjectId) -> AuraResult<Vec<Shot>> {
    let key = project.to_db();
    let rows: Vec<(String, String, Option<String>)> = state.catalog().read(move |conn| {
        let mut query = conn
            .prepare(
                "SELECT p.photo_id, f.file_name, p.capture_time FROM photo p
                 JOIN photo_file f ON f.photo_id = p.photo_id AND f.role = 'primary'
                 WHERE p.project_id = ?1 ORDER BY p.photo_id",
            )
            .map_err(|e| aura_core::errors::db::statement_failed("reference original", &e))?;
        let rows = query
            .query_map([key], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| aura_core::errors::db::statement_failed("reference original", &e))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| aura_core::errors::db::statement_failed("reference original", &e))
    })?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, file, time)| {
            Some(Shot {
                photo: PhotoId::from_db(&id).ok()?,
                file,
                time: time.and_then(|t| {
                    time::OffsetDateTime::parse(&t, &time::format_description::well_known::Rfc3339)
                        .ok()
                }),
            })
        })
        .collect())
}

/// Photographs in the collection that may be this reference's original, best evidence first.
fn candidates<'a>(shots: &'a [Shot], reference: &Path) -> Vec<&'a Shot> {
    let name = reference
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let shot = aura_ingest::exif::extract(reference)
        .ok()
        .and_then(|facts| facts.capture_time);
    let mut ranked: Vec<(u8, &Shot)> = shots
        .iter()
        .filter_map(|s| {
            let by_name = names_match(&name, &s.file);
            let by_time =
                matches!((shot, s.time), (Some(a), Some(b)) if (a - b).whole_seconds().abs() <= 1);
            let rank = match (by_name, by_time) {
                (true, true) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (false, false) => return None,
            };
            Some((rank, s))
        })
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, s)| s)
        .collect()
}

fn resized(image: &Rgb8, width: u32, height: u32) -> Option<Rgb8> {
    let rgb = image::RgbImage::from_raw(image.width, image.height, image.data.clone())?;
    let out = image::imageops::resize(&rgb, width, height, image::imageops::FilterType::Triangle);
    Some(Rgb8 {
        width,
        height,
        data: out.into_raw(),
    })
}

/// Average a frame down to at most `edge` pixels on its longest side.
fn averaged(full: &Frame, edge: u32) -> Frame {
    let longest = full.width.max(full.height).max(1);
    if longest <= edge {
        return full.clone();
    }
    let scale = |v: u32| (u64::from(v) * u64::from(edge) / u64::from(longest)).max(1) as u32;
    let (width, height) = (scale(full.width), scale(full.height));
    let span = |i: u32, out: u32, size: u32| {
        let from = (u64::from(i) * u64::from(size) / u64::from(out)) as usize;
        let to = (u64::from(i + 1) * u64::from(size) / u64::from(out)) as usize;
        from..to.max(from + 1)
    };
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height {
        let rows = span(y, height, full.height);
        for x in 0..width {
            let columns = span(x, width, full.width);
            let (mut sum, mut count) = ([0.0_f32; 3], 0.0_f32);
            for row in rows.clone() {
                let start = (row * full.width as usize + columns.start) * 3;
                let end = (row * full.width as usize + columns.end) * 3;
                for pixel in full.rgb.get(start..end).unwrap_or(&[]).chunks_exact(3) {
                    for (s, v) in sum.iter_mut().zip(pixel) {
                        *s += v;
                    }
                    count += 1.0;
                }
            }
            rgb.extend(sum.map(|v| v / count.max(1.0)));
        }
    }
    Frame::working(rgb, width, height, &full.camera)
}

/// The photograph as every target is matched from - its neutral rendering with the automatic
/// correction, measured as targets measure it - rendered from an averaged frame.
fn baseline(state: &AppState, photo: PhotoId) -> AuraResult<Rgb8> {
    let full =
        crate::photo_frames::CatalogFrames::new(state.clone()).frame(&photo, RenderLevel::Full)?;
    let small = gallery_fit::reduced(&full)?;
    let large = averaged(&full, RENDER_EDGE);
    drop(full);
    let neutral = crate::develop_commands::neutral_recipe(state, photo);
    let source = gallery_fit::render(state, &small, &neutral)?;
    let recipe = crate::edit_profiles::build(
        &neutral,
        None,
        1.0,
        crate::edit_profiles::AutoCorrection::measure(&source.data)?,
        crate::edit_profiles::SceneStats::measure(&source.data)?,
    )?
    .0;
    gallery_fit::render(state, &large, &recipe)
}

/// An edited reference at the size pairs are compared at.
fn edited_image(reference: &Path) -> AuraResult<Rgb8> {
    let edited = super::reference_image::decode(reference)?.image;
    let longest = edited.width.max(edited.height).max(1);
    if longest <= EDGE {
        return Ok(edited);
    }
    let scale = |v: u32| (u64::from(v) * u64::from(EDGE) / u64::from(longest)).max(1) as u32;
    resized(&edited, scale(edited.width), scale(edited.height))
        .ok_or_else(|| super::invalid("The edited reference could not be read."))
}

/// A reference and its original, corresponding pixel for pixel.
struct Pair {
    photo: PhotoId,
    name: String,
    original: Rgb8,
    edited: Rgb8,
}

/// The first candidate that is this reference's original. A refusal is the sentence a
/// photographer is shown.
fn pair_for(
    state: &AppState,
    shots: &[&Shot],
    edited: Rgb8,
    cancel: &CancelToken,
) -> AuraResult<Result<Pair, String>> {
    let aspect = f64::from(edited.width) / f64::from(edited.height.max(1));
    let thumbnail = content::thumbnail(&edited);
    let mut reason = None;
    for shot in shots {
        check_cancel(cancel)?;
        let original = match baseline(state, shot.photo) {
            Ok(image) => image,
            Err(error) => {
                reason.get_or_insert_with(|| {
                    format!(
                        "{} looks like the original, but it could not be rendered ({}).",
                        shot.file, error.user_message
                    )
                });
                continue;
            }
        };
        let theirs = f64::from(original.width) / f64::from(original.height.max(1));
        // A cropped or rotated export is a different framing; its pixels do not line up.
        if (theirs / aspect - 1.0).abs() > 0.02 {
            reason.get_or_insert_with(|| {
                format!(
                    "{} looks like the original, but the edited photo is cropped or rotated, so its pixels do not line up.",
                    shot.file
                )
            });
            continue;
        }
        let Some(original) = resized(&original, edited.width, edited.height) else {
            continue;
        };
        if !content::similarity(&thumbnail, &original).is_some_and(|s| s >= 0.9) {
            reason.get_or_insert_with(|| {
                format!(
                    "{} has the same name or time, but it is a different frame.",
                    shot.file
                )
            });
            continue;
        }
        return Ok(Ok(Pair {
            photo: shot.photo,
            name: shot.file.clone(),
            original,
            edited,
        }));
    }
    Ok(Err(reason.unwrap_or_default()))
}

fn learned(pairs: &[&Pair], model: CalibratedLook, report: CalibrationReport) -> Learned {
    let allowance = pairs
        .iter()
        .map(|p| allowance(&p.original, &p.edited))
        .fold(Allowance::default(), |a, b| Allowance {
            skin: a.skin.max(b.skin),
            clipping: a.clipping.max(b.clipping),
        });
    Learned {
        photos: pairs.iter().map(|p| p.photo).collect(),
        names: pairs.iter().map(|p| p.name.clone()).collect(),
        model,
        report,
        allowance,
    }
}

/// Look for the references' originals in `project` and learn the edit from every pair found.
/// Nothing is learned when no photograph is an original, or when no pair can be explained as one
/// global operation; matching then falls back to the references alone.
pub(crate) fn learn(
    state: &AppState,
    project: &ProjectId,
    references: &[PathBuf],
    cancel: &CancelToken,
) -> AuraResult<Search> {
    let shots = collection(state, project)?;
    let mut search = Search::default();
    let mut pairs = Vec::new();
    for reference in references.iter().take(MAX_REFERENCES) {
        if pairs.len() >= MAX_PAIRS {
            break;
        }
        check_cancel(cancel)?;
        let found = candidates(&shots, reference);
        if found.is_empty() {
            continue;
        }
        // A reference that cannot be read is skipped; the others may still have originals.
        let Ok(edited) = edited_image(reference) else {
            continue;
        };
        match pair_for(state, &found, edited, cancel)? {
            Ok(pair) => pairs.push(pair),
            Err(reason) if !reason.is_empty() => {
                search.refused.get_or_insert(reason);
            }
            Err(_) => {}
        }
    }
    if pairs.is_empty() {
        return Ok(search);
    }
    let all: Vec<&Pair> = pairs.iter().collect();
    match choose(&all, cancel)? {
        Ok((chosen, model, report)) => {
            search.learned = Some(learned(&chosen, model, report));
            search.refused = None;
        }
        Err(reason) => {
            search.refused = Some(format!(
                "{} looks like the original, but {reason}",
                all.first().map_or("", |p| p.name.as_str())
            ));
        }
    }
    Ok(search)
}

type Choice<'a> = (Vec<&'a Pair>, CalibratedLook, CalibrationReport);

/// The pairs to learn from: all of them pooled, which on real photographs of twelve known looks
/// left 11 % of the difference where one pair left 15 %, because more of a batch's colours are
/// covered. One retouched or locally edited reference can keep a pooled fit from qualifying; the
/// pairs that are a global operation on their own are still the photographer's edit, and the one
/// that explains its own reference best is used. A refusal is the calibration's sentence.
fn choose<'a>(all: &[&'a Pair], cancel: &CancelToken) -> AuraResult<Result<Choice<'a>, String>> {
    let fit = |pairs: &[&Pair]| -> AuraResult<Result<(CalibratedLook, CalibrationReport), String>> {
        let images: Vec<(&Rgb8, &Rgb8)> = pairs.iter().map(|p| (&p.original, &p.edited)).collect();
        match calibration::fit_pairs(&images, cancel) {
            Ok(found) => Ok(Ok(found)),
            Err(error) if cancel.is_cancelled() => Err(error),
            Err(error) => Ok(Err(error.user_message)),
        }
    };
    let pooled = fit(all)?;
    let reason = match pooled {
        Ok((model, report)) => return Ok(Ok((all.to_vec(), model, report))),
        Err(reason) => reason,
    };
    let mut best: Option<Choice<'a>> = None;
    if all.len() > 1 {
        for pair in all {
            check_cancel(cancel)?;
            if let Ok((model, report)) = fit(&[*pair])? {
                if best
                    .as_ref()
                    .is_none_or(|(_, _, r)| report.mean_error < r.mean_error)
                {
                    best = Some((vec![*pair], model, report));
                }
            }
        }
    }
    Ok(best.ok_or(reason))
}

#[cfg(test)]
mod tests {
    use super::names_match;

    #[test]
    fn an_export_is_matched_to_its_camera_file_by_name() {
        for (reference, photo) in [
            ("989A2698-Edit.jpg", "989A2698.CR3"),
            ("989A2698.jpg", "989A2698.CR3"),
            ("IMG_1234_lr.JPG", "img_1234.dng"),
            ("DSC0042 (1).jpg", "DSC0042.NEF"),
            ("DSC0042-2.jpg", "DSC0042.ARW"),
        ] {
            assert!(names_match(reference, photo), "{reference} / {photo}");
        }
        for (reference, photo) in [
            ("989A2698.jpg", "989A269.CR3"),
            ("989A26981.jpg", "989A2698.CR3"),
            ("wedding.jpg", "IMG_0001.CR3"),
            ("ab.jpg", "ab.CR3"),
        ] {
            assert!(!names_match(reference, photo), "{reference} / {photo}");
        }
    }
}
