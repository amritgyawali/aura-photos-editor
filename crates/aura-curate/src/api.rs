//! The frozen service, and the pass that fills it.
//!
//! [`CuratePass`] runs the five selectors in the one order they can run in - heroes before the
//! social sets and the teaser, because both read the portfolio's first entry; the album after the
//! coverage report, because coverage is a filter - and writes the whole result in one transaction.
//! A failure leaves the previous curation exactly as it was.
//!
//! [`Curate`] is the `CurateService` implementation the IPC surface holds. It reads; the pass
//! writes; and neither of them changes a photograph.
//!
//! # Why `set_order` is on the service and the reorder check is in `album`
//!
//! Because the check is a property of an album rather than of a store. `album::check_order` is a
//! pure function over the gallery and an order, so the phase gate drives it directly, the cloud
//! validator shares its rule, and a test can assert the refusal without a catalog. The service is
//! where the refusal meets a photographer.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::curate::{
    AlbumPlan, BwPick, CurateOverride, CurateService, CurationOutline, CurationResult,
    ExportFormat, ExportSubject, HeroPick, ImageId, SocialSets, Spread, TeaserPick, ALBUM_MAX,
    ALBUM_MIN,
};
use aura_core::contract::ids::SpreadId;
use aura_core::{AuraError, AuraResult, ProjectId};

use crate::album::{self, Context};
use crate::caption::Vocabulary;
use crate::export::Bundle;
use crate::policy::Policy;
use crate::read::Field;
use crate::sequence::{Applied, SequenceOutput};
use crate::store::{CurateStore, StoredRun};
use crate::{bw, caption, hero, social, teaser};

/// The reading half: what a panel asks.
#[derive(Debug)]
pub struct Curate {
    store: CurateStore,
}

impl Curate {
    /// Open the service over a catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self {
            store: CurateStore::new(catalog, clock),
        }
    }

    /// The store underneath, for the pass and the gate.
    #[must_use]
    pub const fn store(&self) -> &CurateStore {
        &self.store
    }
}

impl CurateService for Curate {
    fn outline(&self, project: ProjectId) -> AuraResult<CurationOutline> {
        self.store.outline(project)
    }

    fn result(&self, project: ProjectId) -> AuraResult<Option<CurationResult>> {
        self.store.result(project)
    }

    fn bw(&self, project: ProjectId) -> AuraResult<Vec<BwPick>> {
        self.store.bw(project)
    }

    fn heroes(&self, project: ProjectId) -> AuraResult<Vec<HeroPick>> {
        self.store.heroes(project)
    }

    fn album(&self, project: ProjectId) -> AuraResult<Option<AlbumPlan>> {
        self.store.album(project)
    }

    fn spread(&self, spread: SpreadId) -> AuraResult<Option<Spread>> {
        self.store.spread(spread)
    }

    fn social(&self, project: ProjectId) -> AuraResult<SocialSets> {
        self.store.social(project)
    }

    fn teaser(&self, project: ProjectId) -> AuraResult<Vec<TeaserPick>> {
        self.store.teaser(project)
    }

    fn set_order(&self, project: ProjectId, order: &[ImageId]) -> Result<(), AuraError> {
        // Checked against the album that exists rather than against the gallery, because an order
        // may only contain frames the album carries: a drag cannot add a photograph.
        let Some(plan) = self.store.album(project)? else {
            return Err(crate::errors::decision_refused(
                "this wedding has no album to reorder; curate it first",
            ));
        };
        let existing = plan.images();
        if order.len() != existing.len() {
            return Err(crate::errors::decision_refused(
                "an album order has to contain exactly the images the album carries; adding or \
                 removing one is a change to the album rather than to its order",
            ));
        }
        let chapters: BTreeMap<ImageId, aura_core::contract::scene::ChapterId> = plan
            .spreads
            .iter()
            .flat_map(|spread| {
                spread
                    .images()
                    .into_iter()
                    .map(move |image| (image, spread.chapter))
            })
            .collect();
        check_order_against(order, &chapters)?;
        self.store.set_order(project, order)
    }

    fn decide(
        &self,
        project: ProjectId,
        image: ImageId,
        decision: CurateOverride,
    ) -> Result<(), AuraError> {
        self.store.decide(project, image, &decision)
    }

    fn export(
        &self,
        project: ProjectId,
        subject: ExportSubject,
        format: ExportFormat,
    ) -> AuraResult<String> {
        let album = self.store.album(project)?;
        let heroes = self.store.heroes(project)?;
        let social = self.store.social(project)?;
        let teaser = self.store.teaser(project)?;
        let bw = self.store.bw(project)?;
        let bundle = Bundle {
            project: project.to_db(),
            album: album.as_ref(),
            heroes: &heroes,
            social: &social,
            teaser: &teaser,
            bw: &bw,
        };
        Ok(crate::export::render(&bundle, subject, format))
    }
}

/// Refuse an order that reorders chapters, names an unknown image, or repeats one.
///
/// The same three refusals as `album::check_order`, over a chapter map rather than over a gallery -
/// which is what the service has after reading a stored album. ADR-0060 section 4.
///
/// # Errors
///
/// `AURA-ML-5143`.
pub fn check_order_against(
    order: &[ImageId],
    chapters: &BTreeMap<ImageId, aura_core::contract::scene::ChapterId>,
) -> Result<(), AuraError> {
    use aura_core::contract::scene::ChapterId;
    let mut seen: std::collections::BTreeSet<ImageId> = std::collections::BTreeSet::new();
    let mut visited: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    let mut last: Option<usize> = None;

    for image in order {
        let Some(chapter) = chapters.get(image) else {
            return Err(crate::errors::decision_refused(format!(
                "{image} is not in this album"
            )));
        };
        if !seen.insert(*image) {
            return Err(crate::errors::decision_refused(format!(
                "{image} appears twice in the order"
            )));
        }
        let Some(position) = ChapterId::ALL.iter().position(|c| c == chapter) else {
            continue;
        };
        if last == Some(position) {
            continue;
        }
        if !visited.insert(position) || last.is_some_and(|previous| position < previous) {
            return Err(crate::errors::chapters_reordered());
        }
        last = Some(position);
    }
    Ok(())
}

/// One curation pass over one project.
#[derive(Debug)]
pub struct CuratePass<'a> {
    field: &'a dyn Field,
    policy: &'a Policy,
    store: &'a CurateStore,
    embed_ver: u16,
}

impl<'a> CuratePass<'a> {
    /// A pass over one project's gallery.
    #[must_use]
    pub const fn new(
        field: &'a dyn Field,
        policy: &'a Policy,
        store: &'a CurateStore,
        embed_ver: u16,
    ) -> Self {
        Self {
            field,
            policy,
            store,
            embed_ver,
        }
    }

    /// Run the whole pass and store what it produced.
    ///
    /// `answer` is a cloud sequencing result, or `None` when the provider was not reached - which is
    /// the common case and produces exactly the album the deterministic optimiser produced.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5144` when a stage cannot run. `AURA-DB-3006` when the result cannot be stored.
    pub fn run(
        &self,
        project: ProjectId,
        target_size: Option<u32>,
        answer: Option<&SequenceOutput>,
    ) -> Result<CurationOutline, AuraError> {
        let frames = self
            .field
            .frames(project)
            .map_err(|e| crate::errors::pass_failed("read", e.detail.clone()))?;
        let photos = self
            .field
            .photo_count(project)
            .map_err(|e| crate::errors::pass_failed("read", e.detail.clone()))?;
        let selected = frames.len() as u32;
        let curated = frames.iter().filter(|f| f.is_readable()).count() as u32;

        // A gallery with nothing in it produces an empty result rather than a failure. A project
        // whose cull has not run is not a curation error; it is a project nobody has culled.
        let target = target_size.unwrap_or(self.policy.album_default).clamp(
            self.policy.album_min.max(ALBUM_MIN),
            self.policy.album_max.min(ALBUM_MAX),
        );

        let loci = self
            .field
            .skin_bands(project)
            .map_err(|e| crate::errors::pass_failed("skin", e.detail.clone()))?;
        let rituals = self
            .field
            .rituals(project)
            .map_err(|e| crate::errors::pass_failed("rituals", e.detail.clone()))?;
        let gallery_coverage = self
            .field
            .gallery_coverage(project)
            .map_err(|e| crate::errors::pass_failed("coverage", e.detail.clone()))?;
        let close_family = self
            .field
            .close_family(project)
            .map_err(|e| crate::errors::pass_failed("family", e.detail.clone()))?;
        let user_order = self
            .store
            .order(project)
            .map_err(|e| crate::errors::pass_failed("order", e.detail.clone()))?;

        let context = Context {
            gallery_coverage,
            close_family,
            user_order,
        };

        // Heroes first: the social sets and the teaser both read the portfolio's first entry, and
        // two answers to "which is the best photograph of this wedding" would be two answers.
        let heroes = hero::select(&frames, self.field, self.policy);
        let bw = bw::candidates(&frames, &loci, self.policy);
        let mut album = album::compose(&frames, &context, self.field, self.policy, target);

        // The cloud can only be agreed with. An absent answer and a refused one produce the same
        // album, which is the whole of ADR-0059 section 11.
        let applied = match answer {
            Some(answer) => {
                crate::sequence::apply(&mut album, answer, &frames, self.field, self.policy)
            }
            None => Applied::default(),
        };

        let vocabulary = Vocabulary::build(&rituals);
        let mut sets = social::build(&frames, &heroes, &vocabulary, self.policy);
        let highlights = social::chapter_highlights(&frames);
        let drafts = answer.map(SequenceOutput::drafts).unwrap_or_default();
        let chapter_captions =
            caption::for_album(&album.chapter_map, &highlights, &vocabulary, &drafts);
        // How many drafts the grounding check refused: a draft that was offered and did not survive.
        let captions_refused = chapter_captions
            .iter()
            .filter(|c| {
                c.source == aura_core::contract::curate::CaptionSource::Template
                    && drafts.contains_key(&c.chapter)
            })
            .count() as u32;
        sets.captions.extend(chapter_captions);
        album.reasons.extend(social::set_reasons(&sets));

        let teaser = teaser::select(&frames, &heroes, self.policy);

        let run = StoredRun {
            project,
            photos,
            selected,
            curated,
            result: CurationResult {
                bw,
                heroes,
                album,
                social: sets,
                teaser,
            },
            cloud_used: answer.is_some(),
            cloud_applied: applied.applied,
            cloud_refused: applied.refused,
            captions_refused,
            policy_ver: self.policy.policy_ver,
            analysis_ver: crate::ANALYSIS_VER,
            embed_ver: self.embed_ver,
        };
        self.store
            .write(&run)
            .map_err(|e| crate::errors::pass_failed("store", e.detail.clone()))?;
        self.store
            .outline(project)
            .map_err(|e| crate::errors::pass_failed("outline", e.detail.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::scene::ChapterId;

    #[test]
    fn an_order_that_reorders_chapters_is_refused_against_a_stored_album_too() {
        let a = ImageId::new();
        let b = ImageId::new();
        let c = ImageId::new();
        let mut chapters = BTreeMap::new();
        chapters.insert(a, ChapterId::Ceremony);
        chapters.insert(b, ChapterId::Ceremony);
        chapters.insert(c, ChapterId::Reception);

        assert!(check_order_against(&[a, b, c], &chapters).is_ok());
        assert!(
            check_order_against(&[b, a, c], &chapters).is_ok(),
            "inside a chapter is fine"
        );

        let err = check_order_against(&[c, a, b], &chapters).unwrap_err();
        assert_eq!(err.code.0, "AURA-ML-5143");

        // Interleaving is the same refusal: a chapter may not be revisited.
        let err = check_order_against(&[a, c, b], &chapters).unwrap_err();
        assert_eq!(err.code.0, "AURA-ML-5143");
    }

    #[test]
    fn an_order_naming_an_image_the_album_does_not_carry_is_refused() {
        let a = ImageId::new();
        let mut chapters = BTreeMap::new();
        chapters.insert(a, ChapterId::Ceremony);
        assert!(check_order_against(&[a, ImageId::new()], &chapters).is_err());
        assert!(check_order_against(&[a, a], &chapters).is_err());
    }
}
