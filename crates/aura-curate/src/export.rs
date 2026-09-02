//! Specifications another tool reads: JSON, CSV and a PSD-ready layer list.
//!
//! Section 2.1: "export to album software formats (JSON/CSV/PSD-ready layer lists) and to social
//! scheduling". Section 2.2 puts album page rendering out of scope, and phase 30 owns delivery.
//!
//! # This produces text, never a file
//!
//! Nothing in this module opens a file handle. `CurateService::export` returns a `String` and the
//! shell saves it, which is what keeps `tests/no_outputs.rs` true and what stops this becoming a
//! second export path beside phase 30's. Two export paths is two answers to what was delivered.
//!
//! # Why the JSON is written by hand
//!
//! A derived serialiser makes the published format a consequence of Rust field names, so renaming
//! `rhythm_measurable` would silently change a format every album-design script in the world had
//! been written against. The keys here are written out in a fixed order and `docs/curation.md`
//! documents them; a change to either is a diff somebody reviews.
//!
//! It is also why [`Version`] exists and is emitted first. A consumer that finds `"version": 2`
//! knows to check its assumptions; one that finds no version at all has to guess.

use std::fmt::Write as _;

use aura_core::contract::curate::{
    album_summary, AlbumPlan, BwPick, ExportFormat, ExportSubject, HeroPick, SocialSets,
    TeaserPick, MIX_BANDS,
};

/// The specification format's own version, emitted as the first key of every JSON export.
///
/// One. Bumped when a key is renamed, removed or given a different meaning; **not** when a key is
/// added, because an added key is invisible to a consumer that does not read it.
pub const VERSION: u32 = 1;

/// Everything one export needs.
#[derive(Debug, Clone)]
pub struct Bundle<'a> {
    /// The project, as its prefixed id.
    pub project: String,
    /// The album, when there is one.
    pub album: Option<&'a AlbumPlan>,
    /// The portfolio.
    pub heroes: &'a [HeroPick],
    /// The social sets.
    pub social: &'a SocialSets,
    /// The teaser.
    pub teaser: &'a [TeaserPick],
    /// The monochrome candidates.
    pub bw: &'a [BwPick],
}

/// Render one subject in one format.
#[must_use]
pub fn render(bundle: &Bundle<'_>, subject: ExportSubject, format: ExportFormat) -> String {
    match (subject, format) {
        (ExportSubject::Album, ExportFormat::Json) => album_json(bundle),
        (ExportSubject::Album, ExportFormat::Csv) => album_csv(bundle),
        (ExportSubject::Album, ExportFormat::LayerList) => album_layers(bundle),
        (ExportSubject::Social, ExportFormat::Json) => social_json(bundle),
        (ExportSubject::Social, ExportFormat::Csv) => social_csv(bundle),
        (ExportSubject::Social, ExportFormat::LayerList) => social_layers(bundle),
        (ExportSubject::Heroes, ExportFormat::Json) => heroes_json(bundle),
        (ExportSubject::Heroes, ExportFormat::Csv) => heroes_csv(bundle),
        (ExportSubject::Heroes, ExportFormat::LayerList) => heroes_layers(bundle),
        (ExportSubject::Teaser, ExportFormat::Json) => teaser_json(bundle),
        (ExportSubject::Teaser, ExportFormat::Csv) => teaser_csv(bundle),
        (ExportSubject::Teaser, ExportFormat::LayerList) => teaser_layers(bundle),
    }
}

// ---------------------------------------------------------------------------
// The album
// ---------------------------------------------------------------------------

/// The album as JSON, with a documented key order.
#[must_use]
pub fn album_json(bundle: &Bundle<'_>) -> String {
    let Some(plan) = bundle.album else {
        return format!(
            "{{\"version\":{VERSION},\"project\":{},\"album\":null}}",
            quote(&bundle.project)
        );
    };
    let mut out = String::new();
    let _ = write!(out, "{{\"version\":{VERSION},");
    let _ = write!(out, "\"project\":{},", quote(&bundle.project));
    let _ = write!(
        out,
        "\"size\":{},\"targetSize\":{},\"spreads\":{},",
        plan.size,
        plan.target_size,
        plan.spreads.len()
    );
    let _ = write!(
        out,
        "\"rhythmScore\":{:.4},\"rhythmMeasurable\":{:.4},\"pairingScore\":{:.4},",
        plan.rhythm_score, plan.rhythm_measurable, plan.pairing_score
    );
    let _ = write!(out, "\"userOrdered\":{},", plan.user_ordered);
    out.push_str("\"chapters\":[");
    for (ix, span) in plan.chapter_map.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"chapter\":{},\"first\":{},\"spreads\":{},\"target\":{}}}",
            quote(span.chapter.as_str()),
            span.first,
            span.len,
            span.target
        );
    }
    out.push_str("],\"pages\":[");
    for (ix, spread) in plan.spreads.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"index\":{},\"chapter\":{},\"single\":{},\"left\":{},\"right\":{},",
            spread.index,
            quote(spread.chapter.as_str()),
            spread.single,
            optional_id(spread.left),
            optional_id(spread.right)
        );
        let _ = write!(
            out,
            "\"pairing\":{{\"score\":{:.4},\"tonalGap\":{:.4},\"warmthGapK\":{:.1},\
             \"facingScore\":{:.4},\"facingKnown\":{},\"similarity\":{:.4}}},",
            spread.pair.score,
            spread.pair.tonal_gap,
            spread.pair.warmth_gap_k,
            spread.pair.facing_score,
            spread.pair.facing_known,
            spread.pair.similarity
        );
        out.push_str("\"reasons\":");
        reasons_json(&mut out, &spread.reasons);
        out.push('}');
    }
    out.push_str("],\"coverage\":{\"mustHaves\":[");
    for (ix, (rule, state)) in plan.coverage.must_haves.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"rule\":{},\"state\":{}}}",
            quote(rule.as_str()),
            quote(state.as_str())
        );
    }
    out.push_str("],\"warnings\":[");
    for (ix, warning) in plan.coverage.warnings.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        out.push_str(&quote(warning));
    }
    out.push_str("]}}");
    out
}

/// The album as CSV: one row per image, which is what a spreadsheet opens.
#[must_use]
pub fn album_csv(bundle: &Bundle<'_>) -> String {
    let mut out =
        String::from("spread,page,chapter,image,single,pairing_score,tonal_gap,facing_known\n");
    let Some(plan) = bundle.album else {
        return out;
    };
    for spread in &plan.spreads {
        for (page, image) in [("left", spread.left), ("right", spread.right)] {
            let Some(image) = image else { continue };
            let _ = writeln!(
                out,
                "{},{},{},{},{},{:.4},{:.4},{}",
                spread.index,
                page,
                csv(spread.chapter.as_str()),
                image.to_db(),
                u8::from(spread.single),
                spread.pair.score,
                spread.pair.tonal_gap,
                u8::from(spread.pair.facing_known)
            );
        }
    }
    out
}

/// The album as a layer list, in stacking order, for a PSD template script.
///
/// One line per placed image, prefixed by its spread and page. A template script reads this
/// top-to-bottom and drops each image into the named slot; the format is deliberately flat, because
/// section 2.2 puts page layout out of scope and a nested format would be an invitation to specify
/// one.
#[must_use]
pub fn album_layers(bundle: &Bundle<'_>) -> String {
    let mut out = String::new();
    let Some(plan) = bundle.album else {
        return out;
    };
    let _ = writeln!(out, "# aura album layer list v{VERSION}");
    let _ = writeln!(out, "# project {}", bundle.project);
    let _ = writeln!(
        out,
        "# {} images across {} spreads",
        plan.size,
        plan.spreads.len()
    );
    for line in album_summary(plan).lines() {
        let _ = writeln!(out, "# {line}");
    }
    for spread in &plan.spreads {
        for (page, image) in [("L", spread.left), ("R", spread.right)] {
            let Some(image) = image else { continue };
            let _ = writeln!(
                out,
                "spread{:03}/{page}\t{}\t{}",
                spread.index + 1,
                image.to_db(),
                spread.chapter.as_str()
            );
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Social
// ---------------------------------------------------------------------------

/// The social sets as JSON, captions included.
#[must_use]
pub fn social_json(bundle: &Bundle<'_>) -> String {
    let sets = bundle.social;
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"version\":{VERSION},\"project\":{},",
        quote(&bundle.project)
    );
    out.push_str("\"grid\":");
    picks_json(&mut out, &sets.grid);
    out.push_str(",\"story\":");
    picks_json(&mut out, &sets.story);
    out.push_str(",\"hero\":");
    match &sets.hero {
        Some(hero) => picks_json(&mut out, std::slice::from_ref(hero)),
        None => out.push_str("null"),
    }
    out.push_str(",\"captions\":[");
    for (ix, caption) in sets.captions.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"image\":{},\"chapter\":{},\"text\":{},\"source\":{},\"grounded\":{}}}",
            optional_id(caption.image_id),
            quote(caption.chapter.as_str()),
            quote(&caption.text),
            quote(caption.source.as_str()),
            caption.grounded
        );
    }
    out.push_str("],\"unfilled\":[");
    for (ix, (slot, short)) in sets.unfilled_slots().iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"slot\":{},\"short\":{short}}}",
            quote(slot.as_str())
        );
    }
    out.push_str("]}");
    out
}

/// The social sets as CSV.
#[must_use]
pub fn social_csv(bundle: &Bundle<'_>) -> String {
    let mut out = String::from("set,rank,slot,image,aspect,legibility,caption\n");
    let sets = bundle.social;
    let caption_of = |image: aura_core::contract::curate::ImageId| -> String {
        sets.captions
            .iter()
            .find(|c| c.image_id == Some(image))
            .map_or_else(String::new, |c| csv(&c.text))
    };
    for (name, picks) in [("grid", &sets.grid), ("story", &sets.story)] {
        for (rank, pick) in picks.iter().enumerate() {
            let _ = writeln!(
                out,
                "{name},{rank},{},{},{},{:.4},{}",
                csv(pick.slot.as_str()),
                pick.image_id.to_db(),
                csv(pick.aspect.as_str()),
                pick.legibility,
                caption_of(pick.image_id)
            );
        }
    }
    if let Some(hero) = &sets.hero {
        let _ = writeln!(
            out,
            "hero,0,{},{},{},{:.4},{}",
            csv(hero.slot.as_str()),
            hero.image_id.to_db(),
            csv(hero.aspect.as_str()),
            hero.legibility,
            caption_of(hero.image_id)
        );
    }
    out
}

/// The social sets as a layer list.
#[must_use]
pub fn social_layers(bundle: &Bundle<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# aura social layer list v{VERSION}");
    let _ = writeln!(out, "# project {}", bundle.project);
    for (name, picks) in [
        ("grid", &bundle.social.grid),
        ("story", &bundle.social.story),
    ] {
        for (rank, pick) in picks.iter().enumerate() {
            let _ = writeln!(
                out,
                "{name}/{:02}\t{}\t{}\t{}",
                rank + 1,
                pick.image_id.to_db(),
                pick.aspect.as_str(),
                pick.slot.as_str()
            );
        }
    }
    if let Some(hero) = &bundle.social.hero {
        let _ = writeln!(
            out,
            "hero/01\t{}\t{}\t{}",
            hero.image_id.to_db(),
            hero.aspect.as_str(),
            hero.slot.as_str()
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Heroes, teaser and monochrome
// ---------------------------------------------------------------------------

/// The portfolio as JSON.
#[must_use]
pub fn heroes_json(bundle: &Bundle<'_>) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"version\":{VERSION},\"project\":{},\"heroes\":[",
        quote(&bundle.project)
    );
    for (ix, hero) in bundle.heroes.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"rank\":{},\"image\":{},\"score\":{:.4},\"chapter\":{},\"scale\":{},\
             \"binding\":{},\"confidence\":{:.4},\"terms\":{{\"technical\":{:.4},\
             \"emotion\":{:.4},\"composition\":{:.4},\"uniqueness\":{:.4},\"story\":{:.4}}},",
            hero.rank,
            quote(&hero.image_id.to_db()),
            hero.score,
            quote(hero.chapter.as_str()),
            quote(hero.scale.as_str()),
            quote(hero.binding.as_str()),
            hero.confidence,
            hero.terms.technical,
            hero.terms.emotion,
            hero.terms.composition,
            hero.terms.uniqueness,
            hero.terms.story
        );
        out.push_str("\"reasons\":");
        reasons_json(&mut out, &hero.reasons);
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// The portfolio as CSV.
#[must_use]
pub fn heroes_csv(bundle: &Bundle<'_>) -> String {
    let mut out = String::from(
        "rank,image,score,confidence,chapter,scale,binding,technical,emotion,composition,\
         uniqueness,story\n",
    );
    for hero in bundle.heroes {
        let _ = writeln!(
            out,
            "{},{},{:.4},{:.4},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4}",
            hero.rank,
            hero.image_id.to_db(),
            hero.score,
            hero.confidence,
            csv(hero.chapter.as_str()),
            csv(hero.scale.as_str()),
            csv(hero.binding.as_str()),
            hero.terms.technical,
            hero.terms.emotion,
            hero.terms.composition,
            hero.terms.uniqueness,
            hero.terms.story
        );
    }
    out
}

/// The portfolio as a layer list.
#[must_use]
pub fn heroes_layers(bundle: &Bundle<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# aura portfolio layer list v{VERSION}");
    let _ = writeln!(out, "# project {}", bundle.project);
    for hero in bundle.heroes {
        let _ = writeln!(
            out,
            "hero/{:02}\t{}\t{}",
            hero.rank + 1,
            hero.image_id.to_db(),
            hero.chapter.as_str()
        );
    }
    out
}

/// The teaser as JSON.
#[must_use]
pub fn teaser_json(bundle: &Bundle<'_>) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"version\":{VERSION},\"project\":{},\"teaser\":[",
        quote(&bundle.project)
    );
    for (ix, pick) in bundle.teaser.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"rank\":{},\"image\":{},\"slot\":{},",
            pick.rank,
            quote(&pick.image_id.to_db()),
            quote(pick.slot.as_str())
        );
        out.push_str("\"reasons\":");
        reasons_json(&mut out, &pick.reasons);
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// The teaser as CSV.
#[must_use]
pub fn teaser_csv(bundle: &Bundle<'_>) -> String {
    let mut out = String::from("rank,image,slot\n");
    for pick in bundle.teaser {
        let _ = writeln!(
            out,
            "{},{},{}",
            pick.rank,
            pick.image_id.to_db(),
            csv(pick.slot.as_str())
        );
    }
    out
}

/// The teaser as a layer list.
#[must_use]
pub fn teaser_layers(bundle: &Bundle<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# aura teaser layer list v{VERSION}");
    let _ = writeln!(out, "# project {}", bundle.project);
    for pick in bundle.teaser {
        let _ = writeln!(
            out,
            "teaser/{:02}\t{}\t{}",
            pick.rank + 1,
            pick.image_id.to_db(),
            pick.slot.as_str()
        );
    }
    out
}

/// The monochrome candidates as CSV, mix included.
///
/// The one export with the eight band values on it, because a photographer taking a mix into another
/// editor needs the numbers rather than a note that a mix exists. Not reachable from
/// `ExportSubject` - it is what the B&W panel's own copy-to-clipboard uses - and exposed here so the
/// format lives beside the others.
#[must_use]
pub fn bw_csv(bundle: &Bundle<'_>) -> String {
    let mut out = String::from("image,score,confidence");
    for band in MIX_BANDS {
        let _ = write!(out, ",{band}");
    }
    out.push('\n');
    for pick in bundle.bw {
        let _ = write!(
            out,
            "{},{:.4},{:.4}",
            pick.image_id.to_db(),
            pick.score,
            pick.confidence
        );
        for value in pick.mix.bands {
            let _ = write!(out, ",{value}");
        }
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn picks_json(out: &mut String, picks: &[aura_core::contract::curate::SocialPick]) {
    out.push('[');
    for (ix, pick) in picks.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"image\":{},\"slot\":{},\"aspect\":{},\"legibility\":{:.4},",
            quote(&pick.image_id.to_db()),
            quote(pick.slot.as_str()),
            quote(pick.aspect.as_str()),
            pick.legibility
        );
        out.push_str("\"reasons\":");
        reasons_json(out, &pick.reasons);
        out.push('}');
    }
    out.push(']');
}

fn reasons_json(out: &mut String, reasons: &[aura_core::contract::curate::CurateReason]) {
    out.push('[');
    for (ix, reason) in reasons.iter().enumerate() {
        if ix > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"code\":{},\"text\":{},\"weight\":{:.4}}}",
            quote(reason.code.as_str()),
            quote(&reason.text),
            reason.weight
        );
    }
    out.push(']');
}

fn optional_id(id: Option<aura_core::contract::curate::ImageId>) -> String {
    id.map_or_else(|| "null".to_string(), |id| quote(&id.to_db()))
}

/// A JSON string literal, with the six escapes the grammar requires.
///
/// Hand-written because the whole format is hand-written, and because the reasons carry sentences a
/// product manager writes - which is the one place in this module where a quote or a backslash can
/// actually appear.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A CSV field, quoted when it has to be.
fn csv(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::cull::{Coverage, CoverageReport, MustHave};
    use aura_core::contract::curate::{
        AspectVariant, Caption, CaptionSource, ChapterSpan, CurateCode, CurateReason, HeroBinding,
        HeroTerms, ImageId, ShotScale, SocialPick, SocialSlot, Spread, SpreadPair,
    };
    use aura_core::contract::ids::SpreadId;
    use aura_core::contract::scene::ChapterId;

    fn plan() -> AlbumPlan {
        let left = ImageId::new();
        let right = ImageId::new();
        let mut out = AlbumPlan::empty(80);
        out.spreads = vec![Spread {
            id: SpreadId::new(),
            index: 0,
            left: Some(left),
            right: Some(right),
            single: false,
            chapter: ChapterId::Ceremony,
            pair: SpreadPair {
                tonal_gap: 0.05,
                warmth_gap_k: 120.0,
                facing_score: 0.0,
                facing_known: false,
                similarity: 0.4,
                score: 0.71,
            },
            reasons: vec![CurateReason::detailed(
                CurateCode::SpreadPaired,
                "these two work together, \"across\" the fold\nreally",
                0.71,
            )],
        }];
        out.chapter_map = vec![ChapterSpan {
            chapter: ChapterId::Ceremony,
            first: 0,
            len: 1,
            target: 2,
        }];
        out.coverage = CoverageReport {
            must_haves: vec![
                (MustHave::Kiss, Coverage::Covered),
                (MustHave::Cake, Coverage::Missing),
            ],
            identity_coverage: Vec::new(),
            chapter_counts: Vec::new(),
            warnings: vec!["the album has no photograph of the cake".into()],
        };
        out.size = 2;
        out.rhythm_score = 0.5;
        out.rhythm_measurable = 0.08;
        out.pairing_score = 0.71;
        out
    }

    fn hero() -> HeroPick {
        HeroPick {
            image_id: ImageId::new(),
            rank: 0,
            score: 0.88,
            terms: HeroTerms {
                technical: 0.9,
                emotion: 0.95,
                composition: 0.8,
                uniqueness: 0.7,
                story: 0.6,
            },
            chapter: ChapterId::Ceremony,
            moment: None,
            scale: ShotScale::Tight,
            binding: HeroBinding::MomentExhausted,
            reasons: vec![CurateReason::plain(CurateCode::EmotionalPeak, 0.95)],
            confidence: 0.82,
            accepted: None,
        }
    }

    fn sets() -> SocialSets {
        let image = ImageId::new();
        SocialSets {
            grid: vec![SocialPick {
                image_id: image,
                aspect: AspectVariant::Square,
                slot: SocialSlot::Hero,
                legibility: 0.77,
                reasons: vec![CurateReason::plain(CurateCode::ThumbnailLegible, 0.77)],
                accepted: None,
            }],
            story: Vec::new(),
            hero: None,
            captions: vec![Caption {
                image_id: Some(image),
                chapter: ChapterId::Ceremony,
                text: "the ceremony".into(),
                source: CaptionSource::Template,
                grounded: true,
            }],
        }
    }

    fn bundle<'a>(
        album: &'a AlbumPlan,
        heroes: &'a [HeroPick],
        social: &'a SocialSets,
        teaser: &'a [TeaserPick],
        bw: &'a [BwPick],
    ) -> Bundle<'a> {
        Bundle {
            project: "prj_00000000-0000-0000-0000-000000000001".into(),
            album: Some(album),
            heroes,
            social,
            teaser,
            bw,
        }
    }

    /// A minimal JSON well-formedness check: balanced braces and brackets outside strings, and a
    /// string that terminates. Not a parser - the point is to catch an unescaped quote or a missing
    /// comma, which is what a hand-written serialiser gets wrong.
    fn json_is_balanced(text: &str) -> bool {
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;
        for ch in text.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                '{' | '[' => depth += 1,
                '}' | ']' => {
                    depth -= 1;
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        depth == 0 && !in_string
    }

    #[test]
    fn every_subject_in_every_format_produces_something() {
        let album = plan();
        let heroes = vec![hero()];
        let social = sets();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let bundle = bundle(&album, &heroes, &social, &teaser, &bw);
        for subject in ExportSubject::ALL {
            for format in ExportFormat::ALL {
                let text = render(&bundle, subject, format);
                assert!(
                    !text.is_empty(),
                    "{subject:?} as {format:?} produced nothing"
                );
                if format == ExportFormat::Json {
                    assert!(
                        json_is_balanced(&text),
                        "{subject:?} as JSON is not balanced:\n{text}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_sentence_with_a_quote_and_a_newline_in_it_does_not_break_the_json() {
        // The reasons carry sentences a product manager writes, which is the one place a quote or a
        // backslash actually appears in this format.
        let album = plan();
        let heroes: Vec<HeroPick> = Vec::new();
        let social = SocialSets::default();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let text = album_json(&bundle(&album, &heroes, &social, &teaser, &bw));
        assert!(json_is_balanced(&text), "{text}");
        assert!(text.contains("\\\"across\\\""));
        assert!(text.contains("\\n"));
    }

    #[test]
    fn the_json_carries_its_own_version_first() {
        let album = plan();
        let heroes: Vec<HeroPick> = Vec::new();
        let social = SocialSets::default();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let b = bundle(&album, &heroes, &social, &teaser, &bw);
        for subject in ExportSubject::ALL {
            let text = render(&b, subject, ExportFormat::Json);
            assert!(
                text.starts_with(&format!("{{\"version\":{VERSION}")),
                "{subject:?}: {text}"
            );
        }
    }

    #[test]
    fn the_album_json_reports_both_rhythm_numbers() {
        // A rhythm of 0.5 measured over 8 % of an album is not a claim about the album, and a
        // consumer that only saw the score would have no way to know.
        let album = plan();
        let heroes: Vec<HeroPick> = Vec::new();
        let social = SocialSets::default();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let text = album_json(&bundle(&album, &heroes, &social, &teaser, &bw));
        assert!(text.contains("\"rhythmScore\":0.5000"));
        assert!(text.contains("\"rhythmMeasurable\":0.0800"));
    }

    #[test]
    fn the_album_csv_has_one_row_per_image_and_a_header() {
        let album = plan();
        let heroes: Vec<HeroPick> = Vec::new();
        let social = SocialSets::default();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let text = album_csv(&bundle(&album, &heroes, &social, &teaser, &bw));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "a header and two images");
        assert!(lines[0].starts_with("spread,page,chapter,image"));
        let columns = lines[0].split(',').count();
        for line in &lines[1..] {
            assert_eq!(line.split(',').count(), columns, "{line}");
        }
    }

    #[test]
    fn a_project_with_no_album_exports_a_null_rather_than_a_broken_document() {
        let heroes: Vec<HeroPick> = Vec::new();
        let social = SocialSets::default();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let empty = Bundle {
            project: "prj_x".into(),
            album: None,
            heroes: &heroes,
            social: &social,
            teaser: &teaser,
            bw: &bw,
        };
        let text = album_json(&empty);
        assert!(json_is_balanced(&text), "{text}");
        assert!(text.contains("\"album\":null"));
        assert_eq!(album_csv(&empty).lines().count(), 1, "the header only");
    }

    #[test]
    fn the_layer_list_names_a_slot_per_placed_image() {
        let album = plan();
        let heroes: Vec<HeroPick> = Vec::new();
        let social = SocialSets::default();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let text = album_layers(&bundle(&album, &heroes, &social, &teaser, &bw));
        let placed: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(placed.len(), 2);
        assert!(placed[0].starts_with("spread001/L\t"));
        assert!(placed[1].starts_with("spread001/R\t"));
        for line in placed {
            assert_eq!(line.split('\t').count(), 3, "{line}");
        }
    }

    #[test]
    fn the_layer_list_carries_the_summary_as_comments_rather_than_as_data() {
        let album = plan();
        let heroes: Vec<HeroPick> = Vec::new();
        let social = SocialSets::default();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let text = album_layers(&bundle(&album, &heroes, &social, &teaser, &bw));
        let comments: Vec<&str> = text.lines().filter(|l| l.starts_with('#')).collect();
        assert!(
            comments.iter().any(|l| l.contains("too little to report")),
            "the summary must say the rhythm is not worth reading: {comments:?}"
        );
        // And it is a comment, so a template script that reads placements does not trip over it.
        assert!(!text
            .lines()
            .any(|l| !l.starts_with('#') && l.contains("too little")));
    }

    #[test]
    fn a_caption_with_a_comma_in_it_is_quoted_in_the_csv() {
        let album = plan();
        let heroes: Vec<HeroPick> = Vec::new();
        let mut social = sets();
        if let Some(caption) = social.captions.first_mut() {
            caption.text = "the ceremony, and the vows".into();
        }
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let text = social_csv(&bundle(&album, &heroes, &social, &teaser, &bw));
        assert!(text.contains("\"the ceremony, and the vows\""), "{text}");
        for line in text.lines().skip(1) {
            // Seven columns, and the caption's comma must not have become an eighth.
            let outside = line
                .chars()
                .scan(false, |in_quotes, ch| {
                    if ch == '"' {
                        *in_quotes = !*in_quotes;
                    }
                    Some((ch, *in_quotes))
                })
                .filter(|(ch, in_quotes)| *ch == ',' && !*in_quotes)
                .count();
            assert_eq!(outside, 6, "{line}");
        }
    }

    #[test]
    fn the_monochrome_export_carries_the_eight_bands_by_name() {
        let album = plan();
        let heroes: Vec<HeroPick> = Vec::new();
        let social = SocialSets::default();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let text = bw_csv(&bundle(&album, &heroes, &social, &teaser, &bw));
        for band in MIX_BANDS {
            assert!(text.contains(band), "{band} missing from {text}");
        }
    }

    #[test]
    fn every_export_is_the_same_twice() {
        let album = plan();
        let heroes = vec![hero()];
        let social = sets();
        let teaser: Vec<TeaserPick> = Vec::new();
        let bw: Vec<BwPick> = Vec::new();
        let b = bundle(&album, &heroes, &social, &teaser, &bw);
        for subject in ExportSubject::ALL {
            for format in ExportFormat::ALL {
                assert_eq!(
                    render(&b, subject, format),
                    render(&b, subject, format),
                    "{subject:?}/{format:?}"
                );
            }
        }
    }
}
