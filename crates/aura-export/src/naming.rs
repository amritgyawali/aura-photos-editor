//! File naming: token substitution, slugification, and collision resolution over a whole job.
//!
//! ## The plan is made before anything is written
//!
//! [`plan`] takes the whole job and returns every file's name. It does not write, it does not
//! render, and it is what `export_preview_names` shows a photographer *before* they commit a
//! wedding to a template. A writer that resolved collisions as it went would be a writer whose
//! answer to "what will these files be called" is "run it and see".
//!
//! ## Two cameras produce the same original name on every real wedding
//!
//! `DSC_0431.NEF` from body A and `DSC_0431.NEF` from body B is not an edge case; it is what
//! happens whenever two Nikons shoot the same day. Section 10.1 asks for collision-free names
//! across 4,000 files "including duplicate original names from two cameras", and the resolution is
//! a numeral: `DSC_0431.jpg`, `DSC_0431_2.jpg`, `DSC_0431_3.jpg`.
//!
//! A silent overwrite here delivers 3,998 files out of 4,000 and reports success, which is the
//! failure section 12's first row is about wearing different clothes.
//!
//! ## Slugification is deliberately aggressive
//!
//! A couple called "Álex & Sam O'Neill" becomes `alex-and-sam-oneill`. Not because the characters
//! are unrepresentable - every filesystem this product runs on takes UTF-8 - but because a
//! delivered gallery is unzipped on a client's Windows laptop, uploaded to a gallery service,
//! synced to a phone and emailed, and each of those has its own opinion about `&`, `'` and
//! normalisation form. The one that bites is `:` on Windows, which is why
//! [`aura_core::NamingTemplate::parse`] refuses it in the template as well.

use std::collections::BTreeSet;
use std::path::PathBuf;

use aura_core::contract::delivery::{
    DeliveryCode, DeliveryReason, ExportJob, ExportSet, ImageId, NameToken, NamingTemplate,
};
use aura_core::AuraResult;

use crate::errors::name_exhausted;
use crate::read::{Field, Frame};

/// How many suffixes a colliding base name may try before the job gives up.
///
/// A thousand. A template with neither `{seq}` nor `{original}` over a 4,000-frame set will exhaust
/// it, which is the case `AURA-RENDER-8025` exists for and which
/// [`aura_core::NamingTemplate::is_distinguishing`] warns about before the job runs.
pub const MAX_SUFFIX: u32 = 1000;

/// The longest a produced file stem may be, in characters.
///
/// Ninety, which leaves room inside the 255-byte limit every filesystem this product runs on shares
/// for a four-character extension, a `_999` suffix, and a destination path a photographer chose.
/// Phase 01 already has `AURA-IO-1005` for a path that is too deep; this is the half of that
/// problem an exporter creates rather than finds.
pub const MAX_STEM: usize = 90;

/// One file's name, decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedName {
    /// The photograph.
    pub image: ImageId,
    /// Which set.
    pub set: String,
    /// Where it lands, relative to the destination root.
    pub rel_path: PathBuf,
    /// Whether a collision suffix was appended.
    pub renamed: bool,
    /// What the panel says about this name.
    pub reasons: Vec<DeliveryReason>,
}

/// Turn text into a filename-safe slug: lower case, ASCII, hyphen separated.
///
/// `&` becomes `and` rather than disappearing, because "Alex & Sam" collapsing to `alex-sam` reads
/// as two words rather than as a couple, and because a gallery of four thousand files is a place a
/// photographer scans rather than reads.
#[must_use]
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_sep = false;
    for ch in text.chars() {
        let mapped: Option<char> = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => ch.to_lowercase().next(),
            // Latin-1 vowels a wedding's couple field actually contains. Not a full
            // transliteration table: what is not mapped becomes a separator, which is correct for
            // a script this list does not cover and is why the fallback below exists.
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => {
                Some('a')
            }
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => Some('e'),
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => Some('i'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => Some('o'),
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => Some('u'),
            'ñ' | 'Ñ' => Some('n'),
            'ç' | 'Ç' => Some('c'),
            '&' => {
                if !out.is_empty() {
                    pending_sep = true;
                }
                for c in "and".chars() {
                    if pending_sep {
                        out.push('-');
                        pending_sep = false;
                    }
                    out.push(c);
                }
                pending_sep = true;
                continue;
            }
            // An apostrophe joins rather than separates: O'Neill is one word.
            '\'' | '\u{2019}' => continue,
            _ => None,
        };
        match mapped {
            Some(c) => {
                if pending_sep && !out.is_empty() {
                    out.push('-');
                }
                pending_sep = false;
                out.push(c);
            }
            None => {
                if !out.is_empty() {
                    pending_sep = true;
                }
            }
        }
    }
    out
}

/// Substitute one template against one frame.
///
/// Returns the stem and the tokens that had nothing to substitute. A token with no value is
/// **dropped**, together with the separator that would have preceded it, rather than replaced with
/// a placeholder: `2026-05-16_alex-and-sam_0031` is the truth and
/// `2026-05-16_alex-and-sam_unknown_0031` is a claim four thousand files carry.
#[must_use]
pub fn substitute(
    template: &NamingTemplate,
    frame: &Frame,
    set: &str,
    couple: Option<&str>,
    sequence: u32,
) -> (String, Vec<NameToken>) {
    let mut out = String::new();
    let mut missing = Vec::new();
    let mut rest = template.as_str();

    // Literal text between tokens is kept as-is; a run of separators left behind by a dropped
    // token is collapsed at the end, which is what stops `a__b` and a trailing underscore.
    while let Some(open) = rest.find('{') {
        out.push_str(rest.get(..open).unwrap_or_default());
        let after = rest.get(open + 1..).unwrap_or_default();
        let Some(close) = after.find('}') else {
            out.push_str(after);
            rest = "";
            break;
        };
        let name = after.get(..close).unwrap_or_default();
        if let Some(token) = NameToken::parse(name) {
            // Four of the seven are slugified here even though the `Field` port documents them
            // as arriving slugified. Defence in depth, and cheap: these values come from a
            // project's couple field, from a chapter name and from EXIF, none of which this
            // product controls, and one of them reaching a path is a delivered gallery written
            // somewhere a photographer did not choose.
            //
            // `{original}` and `{date}` are deliberately *not* slugified. A photographer matches
            // a delivered file to the frame it came from by eye, and `DSC_0431` lower-cased to
            // `dsc-0431` breaks that on every wedding; the date is already a formatted value.
            // Both still go through `sanitise`, which is the half that matters for safety.
            let value: Option<String> = match token {
                NameToken::Date => frame.date.clone(),
                NameToken::Couple => couple.map(slugify),
                NameToken::Chapter => frame.chapter.as_deref().map(slugify),
                NameToken::Sequence => Some(format!("{sequence:04}")),
                NameToken::Camera => frame.camera.as_deref().map(slugify),
                NameToken::Original => frame.original_stem.clone(),
                NameToken::Set => Some(slugify(set)),
            };
            match value {
                Some(v) if !v.is_empty() => out.push_str(&sanitise(&v)),
                _ => missing.push(token),
            }
        }
        rest = after.get(close + 1..).unwrap_or_default();
    }
    out.push_str(rest);

    (tidy(&out), missing)
}

/// Strip anything a filesystem or a client's laptop would object to from a substituted value.
///
/// The template itself is already refused for path separators at parse time; this is for the
/// *values*, which come from EXIF, from a project's couple field, and from a chapter name - none of
/// which this product controls.
fn sanitise(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect()
}

/// Collapse repeated separators, trim them from both ends, and bound the length.
///
/// Dropping a token leaves `a__b`, and a template ending in a dropped token leaves `a_`. Windows
/// additionally refuses a name ending in `.` or a space, which is the sort of thing that produces a
/// gallery that unzips on macOS and fails on the client's laptop.
fn tidy(stem: &str) -> String {
    // A run of two or more dots is collapsed first. `..` in a name that reaches a path join is a
    // parent-directory traversal, and `sanitise` cannot catch it because it maps separators rather
    // than dots - so `../../etc/passwd` arrives here as `..-..-etc-passwd`, which is harmless
    // today and is one `join` refactor away from not being.
    let mut collapsed = String::with_capacity(stem.len());
    let mut dots = 0_usize;
    for ch in stem.chars() {
        if ch == '.' {
            dots += 1;
            continue;
        }
        match dots {
            0 => {}
            1 => collapsed.push('.'),
            _ => collapsed.push('-'),
        }
        dots = 0;
        collapsed.push(ch);
    }
    if dots == 1 {
        collapsed.push('.');
    } else if dots > 1 {
        collapsed.push('-');
    }
    // A leading dot makes a hidden file on two of the three platforms this product runs on, which
    // is a delivered gallery with photographs a client cannot see.
    let stem = collapsed.trim_start_matches('.');

    let mut out = String::with_capacity(stem.len());
    let mut last_sep = true; // leading separators are dropped
    for ch in stem.chars() {
        let is_sep = ch == '_' || ch == '-' || ch == ' ';
        if is_sep {
            if !last_sep {
                out.push(ch);
            }
            last_sep = true;
        } else {
            out.push(ch);
            last_sep = false;
        }
    }
    while out.ends_with('_') || out.ends_with('-') || out.ends_with(' ') || out.ends_with('.') {
        out.pop();
    }
    if out.chars().count() > MAX_STEM {
        out = out.chars().take(MAX_STEM).collect();
        while out.ends_with('_') || out.ends_with('-') || out.ends_with(' ') || out.ends_with('.') {
            out.pop();
        }
    }
    if out.is_empty() {
        // Every token was missing and the template was nothing but tokens. A name is still
        // required, and `image` is the one fact a frame always has.
        out.push_str("image");
    }
    out
}

/// Plan every name in a job, resolving collisions across **the whole job** rather than per set.
///
/// Across the whole job on purpose, and keyed by the whole relative path. A per-set plan would let
/// a second set overwrite a first set's files the day two sets share a directory, while each looked
/// internally consistent; keying by path rather than by name is what stops it suffixing two files
/// that were never going to collide.
///
/// # Errors
///
/// `AURA-RENDER-8025` when a base name has no free suffix within [`MAX_SUFFIX`].
pub fn plan(job: &ExportJob, field: &dyn Field) -> AuraResult<Vec<PlannedName>> {
    let couple = field.couple();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();

    for set in &job.sets {
        let flat = !set.naming.is_distinguishing();
        for (ix, image) in set.images.iter().enumerate() {
            let frame = field.frame(*image);
            let sequence = u32::try_from(ix + 1).unwrap_or(u32::MAX);
            let (stem, missing) =
                substitute(&set.naming, &frame, &set.name, couple.as_deref(), sequence);

            let ext = set.format.extension();
            let dir = slugify(&set.name);
            let (name, renamed) = claim(&mut taken, &dir, &stem, ext)?;

            let mut reasons = Vec::new();
            if renamed {
                reasons.push(DeliveryReason::with(
                    DeliveryCode::NameCollisionResolved,
                    name.clone(),
                ));
            }
            if flat && ix == 0 {
                reasons.push(DeliveryReason::with(
                    DeliveryCode::NamingTemplateNotUnique,
                    set.naming.as_str().to_owned(),
                ));
            }
            if !missing.is_empty() {
                let names: Vec<&str> = missing.iter().map(|t| t.as_str()).collect();
                reasons.push(DeliveryReason::with(
                    DeliveryCode::NameTokenUnavailable,
                    names.join(", "),
                ));
            }

            out.push(PlannedName {
                image: *image,
                set: set.name.clone(),
                rel_path: rel_path(set, &name),
                renamed,
                reasons,
            });
        }
    }
    Ok(out)
}

/// Where a set's files live under the destination root.
///
/// One directory per set, named after the set, and always - even for a single-set job. A gallery
/// and an album in one folder is a folder a photographer has to sort by hand, and a single-set job
/// that wrote to the root would make the two-set case a different shape from the one-set case in
/// the client's zip file.
#[must_use]
pub fn rel_path(set: &ExportSet, file_name: &str) -> PathBuf {
    PathBuf::from(slugify(&set.name)).join(file_name)
}

/// Take a name, suffixing until it is free.
///
/// **Keyed by the whole relative path**, not by the file name. Two sets that write into different
/// directories do not collide - which is the ordinary case, since every set gets a directory of its
/// own - and two that ever write into the same one do. A map keyed by name alone would suffix a
/// perfectly free `album/alex-and-sam_0001.jpg` because a `gallery/` file happened to share a name,
/// which reads as a bug in a photographer's naming template rather than in this function.
///
/// Case-insensitive, because two of the three filesystems this product runs on are, and a gallery
/// that collides only on Windows is a gallery that arrives broken at the client.
fn claim(
    taken: &mut BTreeSet<String>,
    dir: &str,
    stem: &str,
    ext: &str,
) -> AuraResult<(String, bool)> {
    let key = |s: &str| {
        format!(
            "{}/{}.{ext}",
            dir.to_ascii_lowercase(),
            s.to_ascii_lowercase()
        )
    };
    // `insert` returns whether the key was new, which is one lookup instead of two and is also
    // the only form in which the check and the claim cannot drift apart.
    if taken.insert(key(stem)) {
        return Ok((format!("{stem}.{ext}"), false));
    }
    for n in 2..=MAX_SUFFIX {
        let candidate_stem = format!("{stem}_{n}");
        if taken.insert(key(&candidate_stem)) {
            return Ok((format!("{candidate_stem}.{ext}"), true));
        }
    }
    Err(name_exhausted(stem))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::delivery::{
        DeliveryColour, Destination, FileFormat, OutputSharpen, Resize,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf as Pb;

    #[derive(Debug)]
    struct TestField {
        frames: BTreeMap<String, Frame>,
        couple: Option<String>,
    }

    impl Field for TestField {
        fn couple(&self) -> Option<String> {
            self.couple.clone()
        }
        fn photos(&self) -> u32 {
            0
        }
        fn selected(&self) -> u32 {
            0
        }
        fn frame(&self, image: ImageId) -> Frame {
            self.frames.get(&image.to_db()).cloned().unwrap_or_default()
        }
        fn qc_report_path(&self) -> Option<Pb> {
            None
        }
        fn engine_versions(&self) -> Vec<(String, String)> {
            Vec::new()
        }
    }

    fn set_of(name: &str, images: Vec<ImageId>, template: &str) -> ExportSet {
        ExportSet {
            name: name.to_owned(),
            images,
            format: FileFormat::Jpeg,
            quality: 92,
            resize: Resize::Full,
            sharpen: OutputSharpen::Screen,
            naming: NamingTemplate::parse(template).unwrap(),
            colour: DeliveryColour::Srgb,
            bit_depth: 8,
            sidecar: false,
        }
    }

    #[test]
    fn a_couple_becomes_something_every_filesystem_takes() {
        assert_eq!(slugify("Álex & Sam O'Neill"), "alex-and-sam-oneill");
        assert_eq!(slugify("Priya & Rohan"), "priya-and-rohan");
        assert_eq!(slugify("  spaces   here "), "spaces-here");
        assert_eq!(slugify("C:\\Windows"), "c-windows");
    }

    #[test]
    fn two_cameras_that_shot_the_same_original_name_do_not_overwrite_each_other() {
        // The case section 10.1 names. Two Nikons, one wedding, one filename.
        let a = ImageId::new();
        let b = ImageId::new();
        let mut frames = BTreeMap::new();
        for id in [a, b] {
            frames.insert(
                id.to_db(),
                Frame {
                    image: Some(id),
                    original_stem: Some("DSC_0431".to_owned()),
                    ..Frame::default()
                },
            );
        }
        let field = TestField {
            frames,
            couple: None,
        };
        let job = ExportJob::new(
            vec![set_of("gallery", vec![a, b], "{original}")],
            Destination::Folder {
                path: Pb::from("/x"),
            },
        );
        let plan = plan(&job, &field).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].rel_path, Pb::from("gallery/DSC_0431.jpg"));
        assert_eq!(plan[1].rel_path, Pb::from("gallery/DSC_0431_2.jpg"));
        assert!(!plan[0].renamed);
        assert!(plan[1].renamed);
    }

    #[test]
    fn collisions_are_resolved_across_the_whole_job_not_per_set() {
        // Two sets writing into one destination is the ordinary case. Because each set gets its
        // own directory the names do not actually collide, and the plan proves it rather than
        // asserting it.
        let a = ImageId::new();
        let field = TestField {
            frames: BTreeMap::new(),
            couple: Some("Alex and Sam".to_owned()),
        };
        let job = ExportJob::new(
            vec![
                set_of("gallery", vec![a], "{couple}_{seq}"),
                set_of("album", vec![a], "{couple}_{seq}"),
            ],
            Destination::Folder {
                path: Pb::from("/x"),
            },
        );
        let plan = plan(&job, &field).unwrap();
        assert_eq!(plan[0].rel_path, Pb::from("gallery/alex-and-sam_0001.jpg"));
        assert_eq!(plan[1].rel_path, Pb::from("album/alex-and-sam_0001.jpg"));
    }

    #[test]
    fn a_missing_token_is_dropped_together_with_its_separator() {
        let id = ImageId::new();
        let frame = Frame {
            image: Some(id),
            date: Some("2026-05-16".to_owned()),
            chapter: None, // no scene classification on this frame
            ..Frame::default()
        };
        let (stem, missing) = substitute(
            &NamingTemplate::parse("{date}_{chapter}_{seq}").unwrap(),
            &frame,
            "gallery",
            Some("alex-and-sam"),
            31,
        );
        assert_eq!(stem, "2026-05-16_0031");
        assert_eq!(missing, vec![NameToken::Chapter]);
    }

    #[test]
    fn a_template_of_nothing_but_missing_tokens_still_produces_a_name() {
        let (stem, _) = substitute(
            &NamingTemplate::parse("{chapter}{camera}").unwrap(),
            &Frame::default(),
            "gallery",
            None,
            1,
        );
        assert_eq!(stem, "image");
    }

    #[test]
    fn a_value_out_of_exif_cannot_escape_the_destination() {
        // The template is refused for separators at parse time; the *values* are not under this
        // product's control, and a camera field containing a slash is the shape this catches.
        let frame = Frame {
            original_stem: Some("../../etc/passwd".to_owned()),
            ..Frame::default()
        };
        let (stem, _) = substitute(
            &NamingTemplate::parse("{original}").unwrap(),
            &frame,
            "gallery",
            None,
            1,
        );
        assert!(!stem.contains('/'));
        assert!(!stem.contains(".."), "{stem}");
    }

    #[test]
    fn a_flat_template_says_so_once_per_set_rather_than_once_per_file() {
        let ids: Vec<ImageId> = (0..3).map(|_| ImageId::new()).collect();
        let field = TestField {
            frames: BTreeMap::new(),
            couple: Some("Alex and Sam".to_owned()),
        };
        let job = ExportJob::new(
            vec![set_of("gallery", ids, "{couple}")],
            Destination::Folder {
                path: Pb::from("/x"),
            },
        );
        let plan = plan(&job, &field).unwrap();
        let warned = plan
            .iter()
            .filter(|p| {
                p.reasons
                    .iter()
                    .any(|r| r.code == DeliveryCode::NamingTemplateNotUnique)
            })
            .count();
        assert_eq!(warned, 1);
        // ...and every file still gets a name of its own.
        assert_eq!(plan[1].rel_path, Pb::from("gallery/alex-and-sam_2.jpg"));
        assert_eq!(plan[2].rel_path, Pb::from("gallery/alex-and-sam_3.jpg"));
    }

    #[test]
    fn a_collision_is_case_insensitive_because_two_of_three_filesystems_are() {
        let a = ImageId::new();
        let b = ImageId::new();
        let mut frames = BTreeMap::new();
        frames.insert(
            a.to_db(),
            Frame {
                original_stem: Some("IMG_1".to_owned()),
                ..Frame::default()
            },
        );
        frames.insert(
            b.to_db(),
            Frame {
                original_stem: Some("img_1".to_owned()),
                ..Frame::default()
            },
        );
        let field = TestField {
            frames,
            couple: None,
        };
        let job = ExportJob::new(
            vec![set_of("gallery", vec![a, b], "{original}")],
            Destination::Folder {
                path: Pb::from("/x"),
            },
        );
        let plan = plan(&job, &field).unwrap();
        assert!(plan[1].renamed, "img_1 collides with IMG_1 on Windows");
    }

    #[test]
    fn a_stem_is_bounded_so_a_deep_destination_still_fits() {
        let frame = Frame {
            original_stem: Some("x".repeat(400)),
            ..Frame::default()
        };
        let (stem, _) = substitute(
            &NamingTemplate::parse("{original}").unwrap(),
            &frame,
            "gallery",
            None,
            1,
        );
        assert_eq!(stem.chars().count(), MAX_STEM);
    }
}
