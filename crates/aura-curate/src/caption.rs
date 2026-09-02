//! Captions, and the closed vocabulary that makes grounding a property rather than a hope.
//!
//! Section 6.4: "captions are generated from the story graph (chapter, ritual, people roles
//! anonymised) and are grounded - the model may not invent details about the couple". Section 10.1
//! asks for an automated check that they contain "no invented names, places or claims".
//!
//! # Why the check runs the safe way round
//!
//! Every implementation of that as a *filter over bad things* fails. A blocklist of names cannot
//! enumerate names; a regular expression for a venue cannot enumerate venues; and a model asked
//! politely not to invent a place will occasionally invent a place.
//!
//! So [`Vocabulary`] is the closed set of content words a caption **may** contain, built from this
//! project's own labels: the chapter names, the scene names, the rituals phase 07 resolved for this
//! wedding's traditions, and a small set of role words. A caption is accepted when *every* content
//! word in it is in that set. Anything else - a name, a place, a date, a claim about how somebody
//! felt - fails, and failing means the caption is replaced by the template.
//!
//! That makes the same check apply to the local captions and the cloud ones, and it makes the local
//! ones pass by construction because they are assembled **from** the vocabulary.
//!
//! # What it costs
//!
//! The captions this produces are plain. "The ceremony, and the vows" is not copywriting. It is,
//! however, a sentence the product can prove it is entitled to write, and `docs/curation.md` tells a
//! photographer to edit it before posting. ADR-0059 section 10.
//!
//! # What is deliberately not in the vocabulary
//!
//! No names, because this product does not store a person's name as a name. No gendered role words -
//! `bride`, `groom`, `bridesmaid` - because phase 06's rule is that automation never infers which of
//! two people is the bride, and a caption that said it would be inferring it. `couple`, `family`,
//! `friends` and `guests` are what remains, and they are true of every wedding.

use std::collections::BTreeSet;

use aura_core::contract::curate::{
    Caption, CaptionSource, ChapterSpan, CAPTION_MAX_CHARS, CAPTION_MAX_WORDS,
};
use aura_core::contract::scene::{ChapterId, SceneId};

/// Words that carry no facts: articles, conjunctions, prepositions and the handful of neutral
/// connectives a caption needs to be a sentence.
///
/// Fixed rather than derived, and deliberately short. Every addition is a word a model could build a
/// claim out of, so this list is reviewed rather than grown - `before` and `after` are here because
/// a caption about a sequence needs them, and `almost`, `nearly` and `finally` are not, because they
/// are judgements.
const FUNCTION_WORDS: [&str; 30] = [
    "a", "an", "and", "as", "at", "before", "after", "by", "during", "for", "from", "her", "his",
    "in", "into", "its", "of", "on", "one", "the", "their", "then", "there", "this", "to", "under",
    "up", "with", "first", "last",
];

/// The role words a caption may use.
///
/// Four, none of them gendered and none of them a name. Phase 06's rule - automation never infers
/// which of two people is the bride - applied to a sentence somebody might post.
const ROLE_WORDS: [&str; 4] = ["couple", "family", "friends", "guests"];

/// Words that never enter the vocabulary, whatever supplies them.
///
/// This exists because of a defect its own test found, and the defect is worth writing down because
/// the same shape will recur wherever one phase's vocabulary is reused as another's.
///
/// Phase 07's scene vocabulary contains `getting_ready_bride` and `getting_ready_groom`, and
/// [`insert_phrase`] splits a slug on its underscores. So the words `bride` and `groom` arrived in
/// the caption vocabulary through a door nobody opened deliberately, and a caption saying "the
/// bride" would have passed the grounding check - which is precisely the assertion phase 06 forbids
/// automation from making. Its rule is that AURA never assigns `bride` or `groom`, because which of
/// two people is the bride is not a photographic fact.
///
/// Phase 07 is entitled to that scene label: it is a claim about which of two parallel preparation
/// sessions a frame belongs to, made from context, and it stays inside the catalog. A **caption** is
/// a sentence a photographer posts, and the two are not the same claim.
///
/// So the exclusion is applied at the door rather than at the check: a word here cannot enter the
/// vocabulary from a chapter, a scene, a ritual or a role, and [`scene_title`] drops it too so a
/// template can never try to use one.
const EXCLUDED_WORDS: [&str; 8] = [
    "bride",
    "groom",
    "bridesmaid",
    "groomsman",
    "husband",
    "wife",
    "bridal",
    "mrs",
];

/// The closed set of content words one project's captions may contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vocabulary {
    words: BTreeSet<String>,
}

impl Vocabulary {
    /// Build the vocabulary for one project.
    ///
    /// `rituals` is what phase 07 named for **this wedding's** traditions. A wedding with a
    /// saptapadi may have a caption that says `saptapadi`; one without may not, and that is the
    /// difference between a caption grounded in a story graph and one grounded in a list of things
    /// weddings sometimes have.
    #[must_use]
    pub fn build(rituals: &[String]) -> Self {
        let mut words: BTreeSet<String> = BTreeSet::new();
        for word in FUNCTION_WORDS {
            words.insert(word.to_string());
        }
        for word in ROLE_WORDS {
            words.insert(word.to_string());
        }
        for chapter in ChapterId::ALL {
            insert_phrase(&mut words, chapter.as_str());
            insert_phrase(&mut words, chapter_title(chapter));
        }
        for scene in SceneId::ALL {
            insert_phrase(&mut words, scene.as_str());
        }
        for ritual in rituals {
            insert_phrase(&mut words, ritual);
        }
        Self { words }
    }

    /// True when every content word in `text` came from this wedding.
    ///
    /// Punctuation is stripped and case is folded before the comparison, so `Ceremony,` and
    /// `ceremony` are the same word - a check that treated them differently would reject its own
    /// templates.
    #[must_use]
    pub fn grounds(&self, text: &str) -> bool {
        tokens(text).all(|word| self.words.contains(&word))
    }

    /// The words in `text` this wedding did not supply.
    ///
    /// What the refusal reason names, and what a test asserts on. A refusal that could not say which
    /// word failed would be a refusal nobody can debug.
    #[must_use]
    pub fn ungrounded(&self, text: &str) -> Vec<String> {
        tokens(text)
            .filter(|word| !self.words.contains(word))
            .collect()
    }

    /// How many words the vocabulary holds. The number the panel shows beside the grounding note.
    #[must_use]
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// True when the vocabulary is empty, which cannot happen: the function and role words are
    /// always there.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

/// Split a caption into lower-case content words, dropping punctuation.
fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split_whitespace().filter_map(|raw| {
        let cleaned: String = raw
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .flat_map(char::to_lowercase)
            .collect();
        (!cleaned.is_empty()).then_some(cleaned)
    })
}

/// Add every word of a phrase, so `family_portrait` contributes `family` and `portrait`.
///
/// Anything in [`EXCLUDED_WORDS`] is dropped, whichever label supplied it.
fn insert_phrase(words: &mut BTreeSet<String>, phrase: &str) {
    for part in phrase.split(['_', ' ', '-']) {
        let cleaned: String = part
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        if !cleaned.is_empty() && !EXCLUDED_WORDS.contains(&cleaned.as_str()) {
            words.insert(cleaned);
        }
    }
}

/// The words a chapter is called in a caption.
///
/// Not `ChapterId::as_str`, which is a slug: `getting_ready` reads as two words in a sentence and as
/// one in a database. Both go into the vocabulary; this is what the template uses.
#[must_use]
pub const fn chapter_title(chapter: ChapterId) -> &'static str {
    match chapter {
        ChapterId::GettingReady => "getting ready",
        ChapterId::Details => "details",
        ChapterId::Ceremony => "ceremony",
        ChapterId::Rituals => "rituals",
        ChapterId::Portraits => "portraits",
        ChapterId::Reception => "reception",
        ChapterId::Dance => "dance",
        ChapterId::Exit => "exit",
        ChapterId::Other => "the day",
    }
}

/// The words a scene is called in a caption.
///
/// [`EXCLUDED_WORDS`] are dropped, so `getting_ready_bride` reads as `getting ready`. That is a
/// *less* specific caption than phase 07's own label, on purpose: the label is a claim inside the
/// catalog about which preparation session a frame belongs to, and a caption is a sentence a
/// photographer posts about a person.
#[must_use]
pub fn scene_title(scene: SceneId) -> String {
    scene
        .as_str()
        .split('_')
        .filter(|part| !EXCLUDED_WORDS.contains(part))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The local caption for one chapter.
///
/// Assembled from the vocabulary, so it passes [`Vocabulary::grounds`] by construction - and
/// `the_template_grounds_itself` asserts that for every chapter of every tradition rather than
/// trusting the construction.
///
/// `highlight` is the most common scene in that chapter, when there is one. A caption that names it
/// is a better caption and is still only saying something phase 07 already decided.
#[must_use]
pub fn chapter_caption(chapter: ChapterId, highlight: Option<SceneId>) -> String {
    let base = chapter_title(chapter);
    match highlight {
        Some(scene) if scene != SceneId::Unknown && !scene_title(scene).is_empty() => {
            let detail = scene_title(scene);
            let candidate = format!("the {base}, and the {detail}");
            if candidate.chars().count() <= CAPTION_MAX_CHARS
                && candidate.split_whitespace().count() <= CAPTION_MAX_WORDS
            {
                candidate
            } else {
                format!("the {base}")
            }
        }
        _ => format!("the {base}"),
    }
}

/// The local caption for one photograph.
#[must_use]
pub fn image_caption(chapter: ChapterId, scene: Option<SceneId>) -> String {
    match scene {
        Some(scene) if scene != SceneId::Unknown && !scene_title(scene).is_empty() => {
            format!("the {}", scene_title(scene))
        }
        _ => format!("the {}", chapter_title(chapter)),
    }
}

/// Accept a drafted caption, or fall back on the template.
///
/// The one place a cloud draft becomes a stored caption. Three things have to hold: it is grounded,
/// it is inside both bounds, and it is not empty. Anything else and the template is used - which is
/// the property that makes an unreachable provider, a spent budget and a hallucinating model produce
/// the same album.
#[must_use]
pub fn accept(
    drafted: Option<&str>,
    vocabulary: &Vocabulary,
    chapter: ChapterId,
    highlight: Option<SceneId>,
) -> Caption {
    let template = chapter_caption(chapter, highlight);
    let Some(text) = drafted else {
        return Caption {
            image_id: None,
            chapter,
            text: template,
            source: CaptionSource::Template,
            grounded: true,
        };
    };
    let trimmed = text.trim();
    let candidate = Caption {
        image_id: None,
        chapter,
        text: trimmed.to_string(),
        source: CaptionSource::Cloud,
        grounded: true,
    };
    if candidate.within_bounds() && vocabulary.grounds(trimmed) {
        return candidate;
    }
    Caption {
        image_id: None,
        chapter,
        text: template,
        source: CaptionSource::Template,
        grounded: true,
    }
}

/// A caption per chapter of an album, in album order.
#[must_use]
pub fn for_album(
    spans: &[ChapterSpan],
    highlights: &std::collections::BTreeMap<ChapterId, SceneId>,
    vocabulary: &Vocabulary,
    drafted: &std::collections::BTreeMap<ChapterId, String>,
) -> Vec<Caption> {
    spans
        .iter()
        .map(|span| {
            accept(
                drafted.get(&span.chapter).map(String::as_str),
                vocabulary,
                span.chapter,
                highlights.get(&span.chapter).copied(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocabulary() -> Vocabulary {
        Vocabulary::build(&["saptapadi".to_string(), "mehndi".to_string()])
    }

    #[test]
    fn the_template_grounds_itself_for_every_chapter_and_scene() {
        // The property the whole design rests on: the local caption is assembled *from* the
        // vocabulary, so it passes the same check a cloud draft has to pass.
        let vocab = vocabulary();
        for chapter in ChapterId::ALL {
            let plain = chapter_caption(chapter, None);
            assert!(vocab.grounds(&plain), "{plain}");
            for scene in SceneId::ALL {
                let with_scene = chapter_caption(chapter, Some(scene));
                assert!(
                    vocab.grounds(&with_scene),
                    "{with_scene}: {:?}",
                    vocab.ungrounded(&with_scene)
                );
                let caption = Caption {
                    image_id: None,
                    chapter,
                    text: with_scene.clone(),
                    source: CaptionSource::Template,
                    grounded: true,
                };
                assert!(caption.within_bounds(), "{with_scene}");
            }
        }
    }

    #[test]
    fn a_caption_with_a_name_in_it_is_refused() {
        let vocab = vocabulary();
        assert!(!vocab.grounds("Priya and Arjun exchange rings"));
        assert_eq!(
            vocab.ungrounded("Priya and Arjun exchange rings"),
            vec!["priya", "arjun", "exchange"]
        );
    }

    #[test]
    fn a_caption_with_a_place_or_a_date_in_it_is_refused() {
        let vocab = vocabulary();
        assert!(!vocab.grounds("the ceremony at Kathmandu"));
        assert!(!vocab.grounds("the reception on 12 June"));
    }

    #[test]
    fn a_caption_making_a_claim_about_how_anybody_felt_is_refused() {
        let vocab = vocabulary();
        assert!(!vocab.grounds("the couple were overjoyed"));
        assert!(!vocab.grounds("an unforgettable ceremony"));
    }

    #[test]
    fn a_ritual_this_wedding_had_is_allowed_and_one_it_did_not_is_not() {
        let vocab = vocabulary();
        assert!(vocab.grounds("the saptapadi"));
        assert!(
            !vocab.grounds("the hora"),
            "a wedding without a hora may not have a caption that says hora"
        );
    }

    #[test]
    fn punctuation_and_case_do_not_change_the_answer() {
        let vocab = vocabulary();
        assert!(vocab.grounds("The Ceremony, and the vows."));
        assert!(vocab.grounds("the ceremony and the vows"));
    }

    #[test]
    fn no_gendered_role_word_is_in_the_vocabulary() {
        // Phase 06's rule: automation never infers which of two people is the bride, and a caption
        // that said it would be inferring it.
        //
        // This is the test that found the defect `EXCLUDED_WORDS` exists for. Phase 07's scene
        // vocabulary contains `getting_ready_bride`, `insert_phrase` splits a slug on its
        // underscores, and `bride` arrived in the caption vocabulary through a door nobody opened.
        let vocab = vocabulary();
        for word in ["bride", "groom", "bridesmaid", "husband", "wife"] {
            assert!(!vocab.grounds(word), "{word} must not be in the vocabulary");
        }
        assert!(vocab.grounds("the couple"));
        assert!(vocab.grounds("the family"));
    }

    #[test]
    fn a_gendered_scene_label_produces_a_less_specific_caption_rather_than_a_refused_one() {
        // Phase 07 is entitled to its label - it is a claim inside the catalog about which of two
        // parallel preparation sessions a frame belongs to. A caption is a sentence somebody posts
        // about a person, and the two are not the same claim.
        assert_eq!(scene_title(SceneId::GettingReadyBride), "getting ready");
        assert_eq!(scene_title(SceneId::GettingReadyGroom), "getting ready");
        assert_eq!(scene_title(SceneId::FamilyPortrait), "family portrait");

        let vocab = vocabulary();
        let caption = image_caption(ChapterId::GettingReady, Some(SceneId::GettingReadyBride));
        assert!(vocab.grounds(&caption), "{caption}");
        assert!(!caption.contains("bride"));
    }

    #[test]
    fn a_ritual_that_happens_to_contain_an_excluded_word_does_not_reopen_the_door() {
        // A tradition file is editable by a studio, so the exclusion has to hold against a rite
        // name as well as against a scene label.
        let vocab = Vocabulary::build(&["bride_entry".to_string()]);
        assert!(!vocab.grounds("bride"));
        assert!(vocab.grounds("entry"));
    }

    #[test]
    fn a_drafted_caption_that_fails_is_replaced_by_the_template_rather_than_stored() {
        let vocab = vocabulary();
        let refused = accept(
            Some("Priya's ceremony at the lakeside"),
            &vocab,
            ChapterId::Ceremony,
            Some(SceneId::Vows),
        );
        assert_eq!(refused.source, CaptionSource::Template);
        assert!(vocab.grounds(&refused.text));

        let accepted = accept(
            Some("the ceremony and the vows"),
            &vocab,
            ChapterId::Ceremony,
            Some(SceneId::Vows),
        );
        assert_eq!(accepted.source, CaptionSource::Cloud);
        assert_eq!(accepted.text, "the ceremony and the vows");
    }

    #[test]
    fn a_drafted_caption_over_the_word_bound_is_replaced_even_when_grounded() {
        let vocab = vocabulary();
        let long =
            "the ceremony and the vows and the rings and the kiss and the exit and the dance";
        assert!(vocab.grounds(long), "every word is in the vocabulary");
        let result = accept(Some(long), &vocab, ChapterId::Ceremony, None);
        assert_eq!(result.source, CaptionSource::Template);
    }

    #[test]
    fn an_absent_draft_produces_the_template_rather_than_nothing() {
        let vocab = vocabulary();
        let caption = accept(None, &vocab, ChapterId::Dance, Some(SceneId::FirstDance));
        assert_eq!(caption.source, CaptionSource::Template);
        assert!(caption.within_bounds());
        assert!(caption.grounded);
    }

    #[test]
    fn every_stored_caption_is_grounded() {
        // The `grounded = 1` CHECK in migration 29 as a property of this module: there is no path
        // through `accept` that returns a caption this vocabulary does not ground.
        let vocab = vocabulary();
        for draft in [
            Some("Priya and Arjun"),
            Some("the ceremony"),
            Some(""),
            Some("   "),
            None,
        ] {
            let caption = accept(draft, &vocab, ChapterId::Ceremony, Some(SceneId::Kiss));
            assert!(caption.grounded);
            assert!(vocab.grounds(&caption.text), "{}", caption.text);
            assert!(caption.within_bounds(), "{}", caption.text);
        }
    }
}
