//! Four synthetic weddings with a known right answer.
//!
//! PHASE-12 section 10.1 asks for photographer agreement, zero missed must-haves, family
//! coverage, determinism, slider performance and aggressive-mode safety. Every one of those
//! is a claim about a *wedding*, so the gates need weddings - and there are none in this
//! repository.
//!
//! ## What these fixtures prove and what they do not
//!
//! They prove the **algorithms**. The quota formula, the coverage guard, the diversity
//! caps, the sizing reconciliation and the determinism hash are all exercised against
//! inputs whose correct output is known by construction: this wedding has exactly four
//! photographs of the ring exchange, so a gallery with none of them is a failure that can
//! be asserted rather than judged.
//!
//! They prove **nothing about the weights**, and they cannot. The sub-scores here are
//! authored numbers, not readings from phases 09, 10 and 11 - and even in production those
//! three phases ship placeholder heads. That is condition C1 in the exit report, it is a
//! Sev 2 trigger, and the exit report is explicit that no number in this phase is a claim
//! about a real wedding's pixels until phase 05's condition C10 closes.
//!
//! The distinction is the same one phases 06, 09, 10 and 11 make and it is worth making
//! again here because this is the phase where a passing gate is most likely to be read as
//! "the culling is good". It is not. It is "the culling does what it says".
//!
//! ## The four weddings, and why these four
//!
//! * `hindu_night` - a long ritual sequence in bad light, and the wedding that would suffer
//!   most from a Western-trained model. It has the most frames, the most chapters and the
//!   largest ritual chapter, and the `rituals` importance row is what it tests.
//! * `christian_daylight` - the textbook case. Every must-have present, good light, a
//!   heavy portrait session. If anything is wrong with the ordinary path it shows here.
//! * `nepali_reception` - mixed light, an enormous dance chapter and forty guests. This is
//!   the diversity pass's fixture: without it the gallery is a dance floor with a wedding
//!   attached.
//! * `sparse_elopement` - four hundred frames, no cake, no formals, no reception entrance,
//!   no exit. The fixture for the state that is *correct* rather than broken: `Missing`,
//!   reported honestly, on four rules at once.
//!
//! The three reference weddings named in section 14 are the first three; the fourth exists
//! because a gate that only ever sees complete weddings never checks that the product can
//! say "nobody photographed this".

use std::collections::BTreeSet;

use aura_core::contract::cull::ImageId;
use aura_core::contract::emotion::Interaction;
use aura_core::{ChapterId, IdentityId, KeepScore, MomentId, MustHave, SceneId};
use uuid::Uuid;

use crate::input::{Candidate, CullInput, Framing, IdentityInfo, MomentInfo};

/// One synthetic wedding and the answers a gate checks against.
#[derive(Debug, Clone)]
pub struct Wedding {
    /// The slug the label file and the gate output use.
    pub name: &'static str,
    /// Everything the engine sees.
    pub input: CullInput,
    /// Everything the gate knows and the engine does not.
    pub labels: Labels,
}

/// Ground truth for one synthetic wedding.
#[derive(Debug, Clone, Default)]
pub struct Labels {
    /// The frames a photographer would keep.
    ///
    /// The frames a photographer would keep.
    ///
    /// A deliberately simple model of how somebody culls by hand, and every part of it is
    /// chosen to be *independent of the engine's machinery*: no quotas, no coverage
    /// guarantees, no diversity caps, no chapter bands and no size target.
    ///
    /// 1. Take the strongest frame of every moment.
    /// 2. Drop the moments whose strongest frame is in the weakest [`SKIP_WORST`] of its own
    ///    **scene** - a photographer skips the moments where nothing worked, and judges a
    ///    dark ritual frame against other ritual frames rather than against a golden-hour
    ///    portrait.
    /// 3. Add a second frame from every moment that genuinely varied.
    ///
    /// The scene-relative comparison in step 2 is the part that matters. A *global* quality
    /// threshold - the frame-by-frame cull section 1 says every competitor ships - would
    /// delete the entire ritual chapter of `hindu_night` because it was shot in bad light,
    /// and measuring agreement against that would be measuring whether this engine
    /// reproduces the failure it exists to fix.
    ///
    /// Section 10.1 specifies Jaccard at *moment* level, which is what makes the comparison
    /// fair in the other direction: two editors who pick different frames of the same
    /// instant agree about the wedding, and a frame-by-frame measure would call that a
    /// disagreement.
    pub keepers: BTreeSet<ImageId>,
    /// The rules that have at least one candidate in this wedding.
    ///
    /// The gate asserts every one of these ends `Covered` or `CoveredWeak`, and asserts
    /// that every rule *not* in this set ends `Missing`. Both halves matter: the first is
    /// "never lose a must-have" and the second is "never claim coverage you do not have".
    pub must_have_candidates: BTreeSet<MustHave>,
    /// The identities the coverage guard must give at least three frames each.
    pub close_family: Vec<IdentityId>,
}

/// The share of each scene's moments the label model skips entirely.
///
/// A tenth. A photographer does skip moments where nothing worked, and a model that kept
/// one frame from every moment without exception would make the agreement gate impossible
/// to fail - the engine also delivers roughly one frame per moment, so the two sets would
/// coincide by construction and a gate that cannot fail is not a gate.
///
/// A tenth rather than a quarter because the failure this gate is watching for is
/// *over-culling*, and a label model that itself culled hard would hide it.
pub const SKIP_WORST: f32 = 0.10;

/// Every fixture wedding, in a fixed order.
#[must_use]
pub fn all() -> Vec<Wedding> {
    vec![
        hindu_night(),
        christian_daylight(),
        nepali_reception(),
        sparse_elopement(),
    ]
}

/// One fixture by name.
#[must_use]
pub fn by_name(name: &str) -> Option<Wedding> {
    all().into_iter().find(|wedding| wedding.name == name)
}

/// A long ritual sequence in bad light.
#[must_use]
pub fn hindu_night() -> Wedding {
    let mut builder = Builder::new(0x00A1, 9.5);
    builder.identities(2, 6, 22);
    builder.block(
        SceneId::GettingReadyBride,
        ChapterId::GettingReady,
        14,
        5,
        0.62,
    );
    builder.block(
        SceneId::GettingReadyGroom,
        ChapterId::GettingReady,
        9,
        4,
        0.60,
    );
    builder.block(SceneId::Details, ChapterId::Details, 8, 4, 0.70);
    builder.block(SceneId::Venue, ChapterId::Details, 2, 3, 0.66);
    builder.block(SceneId::CeremonyEntrance, ChapterId::Ceremony, 5, 6, 0.55);
    builder.block(SceneId::Ritual, ChapterId::Rituals, 34, 7, 0.52);
    builder.block(SceneId::Vows, ChapterId::Ceremony, 4, 5, 0.56);
    builder.interaction_block(
        SceneId::Ceremony,
        ChapterId::Ceremony,
        3,
        4,
        0.54,
        Interaction::RingExchange,
    );
    builder.block(SceneId::Kiss, ChapterId::Ceremony, 2, 4, 0.58);
    builder.block(SceneId::FamilyPortrait, ChapterId::Portraits, 16, 4, 0.72);
    builder.block(SceneId::CouplePortrait, ChapterId::Portraits, 12, 5, 0.74);
    builder.block(SceneId::ReceptionEntrance, ChapterId::Reception, 3, 5, 0.53);
    builder.block(SceneId::Speeches, ChapterId::Reception, 10, 5, 0.58);
    builder.block(SceneId::Cake, ChapterId::Reception, 3, 4, 0.60);
    builder.block(SceneId::FirstDance, ChapterId::Dance, 6, 6, 0.55);
    builder.block(SceneId::DanceFloor, ChapterId::Dance, 40, 6, 0.50);
    builder.block(SceneId::Candid, ChapterId::Other, 18, 3, 0.57);
    builder.block(SceneId::Exit, ChapterId::Exit, 3, 5, 0.48);
    builder.finish("hindu_night")
}

/// The textbook case: every must-have, good light, a heavy portrait session.
#[must_use]
pub fn christian_daylight() -> Wedding {
    let mut builder = Builder::new(0x00B2, 8.0);
    builder.identities(2, 5, 18);
    builder.block(
        SceneId::GettingReadyBride,
        ChapterId::GettingReady,
        12,
        4,
        0.68,
    );
    builder.block(
        SceneId::GettingReadyGroom,
        ChapterId::GettingReady,
        8,
        4,
        0.66,
    );
    builder.block(SceneId::Details, ChapterId::Details, 10, 4, 0.74);
    builder.block(SceneId::Venue, ChapterId::Details, 3, 3, 0.72);
    builder.block(SceneId::FirstLook, ChapterId::Portraits, 3, 6, 0.70);
    builder.block(SceneId::CeremonyEntrance, ChapterId::Ceremony, 4, 5, 0.64);
    builder.block(SceneId::Ceremony, ChapterId::Ceremony, 14, 5, 0.66);
    builder.block(SceneId::Vows, ChapterId::Ceremony, 4, 4, 0.68);
    builder.block(SceneId::Rings, ChapterId::Ceremony, 3, 5, 0.65);
    builder.block(SceneId::Kiss, ChapterId::Ceremony, 2, 5, 0.69);
    builder.block(SceneId::FamilyPortrait, ChapterId::Portraits, 18, 4, 0.75);
    builder.block(SceneId::GroupPortrait, ChapterId::Portraits, 6, 4, 0.73);
    builder.block(SceneId::CouplePortrait, ChapterId::Portraits, 20, 5, 0.78);
    builder.block(SceneId::GoldenHour, ChapterId::Portraits, 10, 5, 0.80);
    builder.block(SceneId::ReceptionEntrance, ChapterId::Reception, 2, 5, 0.62);
    builder.block(SceneId::Speeches, ChapterId::Reception, 12, 5, 0.64);
    builder.block(SceneId::Cake, ChapterId::Reception, 2, 4, 0.66);
    builder.block(SceneId::FirstDance, ChapterId::Dance, 5, 6, 0.63);
    builder.block(SceneId::DanceFloor, ChapterId::Dance, 26, 6, 0.58);
    builder.block(SceneId::Candid, ChapterId::Other, 14, 3, 0.62);
    builder.block(SceneId::Exit, ChapterId::Exit, 3, 5, 0.56);
    builder.finish("christian_daylight")
}

/// Mixed light and an enormous dance chapter. The diversity pass's fixture.
#[must_use]
pub fn nepali_reception() -> Wedding {
    let mut builder = Builder::new(0x00C3, 7.0);
    builder.identities(2, 7, 40);
    builder.block(SceneId::Details, ChapterId::Details, 6, 4, 0.70);
    builder.block(SceneId::Venue, ChapterId::Details, 2, 3, 0.68);
    builder.block(SceneId::Ritual, ChapterId::Rituals, 12, 6, 0.54);
    builder.block(SceneId::Vows, ChapterId::Ceremony, 3, 4, 0.58);
    builder.interaction_block(
        SceneId::Ceremony,
        ChapterId::Ceremony,
        3,
        4,
        0.56,
        Interaction::RingExchange,
    );
    builder.block(SceneId::Kiss, ChapterId::Ceremony, 2, 4, 0.60);
    builder.block(SceneId::FamilyPortrait, ChapterId::Portraits, 14, 4, 0.71);
    builder.block(SceneId::CouplePortrait, ChapterId::Portraits, 8, 5, 0.73);
    builder.block(SceneId::ReceptionEntrance, ChapterId::Reception, 3, 5, 0.55);
    builder.block(SceneId::Speeches, ChapterId::Reception, 14, 5, 0.57);
    builder.block(SceneId::Cake, ChapterId::Reception, 2, 4, 0.59);
    builder.block(SceneId::FirstDance, ChapterId::Dance, 5, 6, 0.56);
    builder.block(SceneId::DanceFloor, ChapterId::Dance, 70, 6, 0.49);
    builder.block(SceneId::Candid, ChapterId::Other, 22, 3, 0.58);
    builder.block(SceneId::Exit, ChapterId::Exit, 2, 5, 0.50);
    builder.finish("nepali_reception")
}

/// Four hundred frames, and four rules with nothing to satisfy them.
#[must_use]
pub fn sparse_elopement() -> Wedding {
    let mut builder = Builder::new(0x00D4, 3.0);
    builder.identities(2, 2, 5);
    builder.block(SceneId::Details, ChapterId::Details, 5, 3, 0.72);
    builder.block(SceneId::Venue, ChapterId::Details, 2, 3, 0.70);
    builder.block(SceneId::CeremonyEntrance, ChapterId::Ceremony, 2, 4, 0.64);
    builder.block(SceneId::Vows, ChapterId::Ceremony, 3, 4, 0.68);
    builder.block(SceneId::Rings, ChapterId::Ceremony, 2, 4, 0.66);
    builder.block(SceneId::Kiss, ChapterId::Ceremony, 2, 4, 0.70);
    builder.block(SceneId::CouplePortrait, ChapterId::Portraits, 14, 5, 0.76);
    builder.block(SceneId::GoldenHour, ChapterId::Portraits, 8, 5, 0.78);
    builder.block(SceneId::Candid, ChapterId::Other, 10, 3, 0.64);
    builder.finish("sparse_elopement")
}

/// A deterministic wedding builder.
///
/// The pseudo-random source is a 64-bit linear congruential generator seeded per wedding,
/// so every fixture is byte-identical on every machine and every run. That is not a
/// convenience: section 10.1's determinism gate compares two selections, and a fixture that
/// differed between them would make the gate pass for the wrong reason.
struct Builder {
    seed: u64,
    counter: u128,
    clock_ms: i64,
    hours: f32,
    candidates: Vec<Candidate>,
    moments: Vec<MomentInfo>,
    identities: Vec<IdentityInfo>,
    moment_bests: Vec<MomentLabel>,
    rules: BTreeSet<MustHave>,
    close_family: Vec<IdentityId>,
    primaries: Vec<IdentityId>,
    family: Vec<IdentityId>,
    guests: Vec<IdentityId>,
}

impl Builder {
    fn new(seed: u64, hours: f32) -> Self {
        Self {
            seed,
            counter: u128::from(seed) << 64,
            clock_ms: 1_600_000_000_000,
            hours,
            candidates: Vec::new(),
            moments: Vec::new(),
            identities: Vec::new(),
            moment_bests: Vec::new(),
            rules: BTreeSet::new(),
            close_family: Vec::new(),
            primaries: Vec::new(),
            family: Vec::new(),
            guests: Vec::new(),
        }
    }

    /// The next pseudo-random number in `0..1`.
    fn next_unit(&mut self) -> f32 {
        // Numerical Recipes' LCG constants. Any full-period LCG would do; what matters is
        // that it is written here rather than pulled from a crate whose version could
        // change the fixtures under a future build.
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        f32::from(u16::try_from((self.seed >> 48) & 0xFFFF).unwrap_or(0)) / 65_535.0
    }

    fn next_photo(&mut self) -> ImageId {
        self.counter += 1;
        aura_core::PhotoId::from_uuid(Uuid::from_u128(self.counter))
    }

    fn next_moment(&mut self) -> MomentId {
        self.counter += 1;
        MomentId::from_uuid(Uuid::from_u128(self.counter))
    }

    fn next_identity(&mut self) -> IdentityId {
        self.counter += 1;
        IdentityId::from_uuid(Uuid::from_u128(self.counter))
    }

    /// Create the cast: two of the couple, `family` close relatives, `guests` guests.
    fn identities(&mut self, couple: usize, family: usize, guests: usize) {
        for _ in 0..couple {
            let id = self.next_identity();
            self.primaries.push(id);
            self.close_family.push(id);
            self.identities.push(IdentityInfo {
                id,
                weight: 1.0,
                primary: true,
                close_family: true,
                appearances: 0,
            });
        }
        for _ in 0..family {
            let id = self.next_identity();
            self.family.push(id);
            self.close_family.push(id);
            self.identities.push(IdentityInfo {
                id,
                weight: 0.7,
                primary: false,
                close_family: true,
                appearances: 0,
            });
        }
        for _ in 0..guests {
            let id = self.next_identity();
            self.guests.push(id);
            self.identities.push(IdentityInfo {
                id,
                weight: 0.3,
                primary: false,
                close_family: false,
                appearances: 0,
            });
        }
    }

    fn block(
        &mut self,
        scene: SceneId,
        chapter: ChapterId,
        moments: usize,
        frames: usize,
        quality: f32,
    ) {
        self.emit(scene, chapter, moments, frames, quality, None);
    }

    fn interaction_block(
        &mut self,
        scene: SceneId,
        chapter: ChapterId,
        moments: usize,
        frames: usize,
        quality: f32,
        interaction: Interaction,
    ) {
        self.emit(scene, chapter, moments, frames, quality, Some(interaction));
    }

    #[allow(clippy::too_many_lines)]
    fn emit(
        &mut self,
        scene: SceneId,
        chapter: ChapterId,
        moments: usize,
        frames: usize,
        quality: f32,
        interaction: Option<Interaction>,
    ) {
        for _ in 0..moments {
            let moment_id = self.next_moment();
            let diversity = self.next_unit();
            let significance = 0.3 + 0.6 * self.next_unit();
            let cast = self.cast_for(scene);
            let mut best = (f32::MIN, None);
            let mut second = (f32::MIN, None);
            let mut group_frames: Vec<ImageId> = Vec::new();

            for index in 0..frames {
                let image = self.next_photo();
                let jitter = (self.next_unit() - 0.5) * 0.30;
                let technical = (quality + jitter).clamp(0.05, 0.99);
                let emotion = (quality - 0.05 + (self.next_unit() - 0.5) * 0.35).clamp(0.05, 0.99);
                let composition =
                    (quality - 0.02 + (self.next_unit() - 0.5) * 0.28).clamp(0.05, 0.99);
                let prominence = if cast.is_empty() {
                    KeepScore::NEUTRAL
                } else {
                    (0.55 + 0.4 * self.next_unit()).clamp(0.05, 0.99)
                };
                let truth = technical * 0.4 + emotion * 0.3 + composition * 0.2 + prominence * 0.1;

                self.clock_ms += 900;
                let framing = match index % 3 {
                    0 => Framing::Tight,
                    1 => Framing::Medium,
                    _ => Framing::Wide,
                };
                self.candidates.push(Candidate {
                    image_id: image,
                    moment_id: Some(moment_id),
                    chapter,
                    scene,
                    timeline_ts: self.clock_ms,
                    technical,
                    emotion,
                    composition,
                    prominence,
                    analysed: true,
                    has_emotion: true,
                    has_composition: true,
                    veto: None,
                    is_peak: false,
                    peak_flat: false,
                    duplicate_group: None,
                    identities: cast.clone(),
                    framing,
                    interactions: interaction.into_iter().collect(),
                    confidence: 0.80,
                    model_ver: 1,
                });
                group_frames.push(image);
                if truth > best.0 {
                    second = best;
                    best = (truth, Some(image));
                } else if truth > second.0 {
                    second = (truth, Some(image));
                }
            }

            // The peak is the strongest frame, which is what phase 10 would find on a
            // wedding whose action really does have an apex.
            if let Some(image) = best.1 {
                if let Some(candidate) = self
                    .candidates
                    .iter_mut()
                    .find(|candidate| candidate.image_id == image)
                {
                    candidate.is_peak = true;
                }
            }
            if let Some(image) = best.1 {
                self.moment_bests.push(MomentLabel {
                    scene,
                    truth: best.0,
                    best: image,
                    second: second.1,
                    diversity,
                });
            }

            self.moments.push(MomentInfo {
                id: moment_id,
                frames: u32::try_from(frames).unwrap_or(u32::MAX),
                diversity,
                significance,
                duplicate_groups: Vec::new(),
                user_locked: false,
            });
            self.clock_ms += gap_for(scene);
        }

        for rule in rules_for(scene, interaction) {
            self.rules.insert(rule);
        }
        if !self.close_family.is_empty() {
            self.rules.insert(MustHave::CloseFamily);
        }
    }

    /// Who is in a frame of this scene.
    ///
    /// The couple everywhere they should be, family in the formals and the reception, and a
    /// rotating handful of guests on the dance floor - which is the shape that makes the
    /// identity coverage rule and the identity diversity cap both meaningful.
    fn cast_for(&mut self, scene: SceneId) -> Vec<IdentityId> {
        let mut cast = Vec::new();
        match scene {
            SceneId::Details | SceneId::Venue => return cast,
            SceneId::FamilyPortrait => {
                cast.extend(self.primaries.iter().copied());
                cast.extend(self.family.iter().copied());
            }
            SceneId::DanceFloor | SceneId::Speeches | SceneId::Candid => {
                let pick = self.next_unit();
                if !self.guests.is_empty() {
                    let index = (pick * self.guests.len() as f32) as usize;
                    if let Some(guest) = self.guests.get(index.min(self.guests.len() - 1)) {
                        cast.push(*guest);
                    }
                }
                if pick > 0.6 {
                    cast.extend(self.primaries.iter().copied());
                }
            }
            _ => cast.extend(self.primaries.iter().copied()),
        }
        cast
    }

    fn finish(mut self, name: &'static str) -> Wedding {
        // Appearances are counted rather than declared, so the guest-recurrence rule is
        // exercised against the wedding that was actually generated.
        for identity in &mut self.identities {
            identity.appearances = u32::try_from(
                self.candidates
                    .iter()
                    .filter(|candidate| candidate.identities.contains(&identity.id))
                    .count(),
            )
            .unwrap_or(u32::MAX);
        }
        // Measured from the frames rather than declared, so that a change to `gap_for`
        // cannot leave the size model reading an hour count the wedding does not have.
        let first = self
            .candidates
            .iter()
            .map(|candidate| candidate.timeline_ts)
            .min()
            .unwrap_or_default();
        let last = self
            .candidates
            .iter()
            .map(|candidate| candidate.timeline_ts)
            .max()
            .unwrap_or_default();
        let hours = ((last - first) as f32 / 3_600_000.0).max(self.hours * 0.25);

        let mut input = CullInput {
            candidates: self.candidates,
            moments: self.moments,
            identities: self.identities,
            hours,
            couple_unconfirmed: false,
            model_ver: 1,
        };
        input.sort();
        let keepers = label(&self.moment_bests);

        Wedding {
            name,
            input,
            labels: Labels {
                keepers,
                must_have_candidates: self.rules,
                close_family: self.close_family,
            },
        }
    }
}

/// One moment, as the label model sees it.
struct MomentLabel {
    scene: SceneId,
    truth: f32,
    best: ImageId,
    second: Option<ImageId>,
    diversity: f32,
}

/// The frames a photographer would keep. See [`Labels::keepers`].
fn label(moments: &[MomentLabel]) -> BTreeSet<ImageId> {
    // The bar is per scene, so a dark ritual moment is judged against other ritual moments.
    let mut by_scene: std::collections::BTreeMap<SceneId, Vec<f32>> =
        std::collections::BTreeMap::new();
    for moment in moments {
        by_scene.entry(moment.scene).or_default().push(moment.truth);
    }
    let mut floors: std::collections::BTreeMap<SceneId, f32> = std::collections::BTreeMap::new();
    for (scene, mut values) in by_scene {
        values.sort_by(f32::total_cmp);
        let index = ((values.len() as f32) * SKIP_WORST) as usize;
        let floor = values
            .get(index.min(values.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0);
        floors.insert(scene, floor);
    }

    let mut keepers = BTreeSet::new();
    for moment in moments {
        if moment.truth < floors.get(&moment.scene).copied().unwrap_or(0.0) {
            continue;
        }
        keepers.insert(moment.best);
        if moment.diversity > 0.6 {
            if let Some(second) = moment.second {
                keepers.insert(second);
            }
        }
    }
    keepers
}

/// How far apart two moments of this scene sit, in milliseconds.
///
/// Not decoration. The diversity pass caps delivered frames per chapter per two-minute
/// window, so a fixture whose whole wedding happened in twenty minutes would trip that cap
/// everywhere and a fixture whose moments were all four minutes apart would never trip it
/// at all - and in both cases the pass would be untested while appearing to be tested.
///
/// The numbers are how a wedding is actually shot. A dance floor produces a burst every
/// twenty seconds; a ceremony produces one every minute and a half; portraits sit between
/// them; and the set pieces - the entrances, the cake, the exit - happen once and are two
/// minutes from anything else.
fn gap_for(scene: SceneId) -> i64 {
    match scene {
        SceneId::DanceFloor => 20_000,
        SceneId::CouplePortrait | SceneId::GoldenHour | SceneId::Details => 45_000,
        SceneId::Candid | SceneId::FamilyPortrait | SceneId::GroupPortrait => 60_000,
        SceneId::GettingReadyBride
        | SceneId::GettingReadyGroom
        | SceneId::Ritual
        | SceneId::Ceremony
        | SceneId::Speeches => 90_000,
        _ => 120_000,
    }
}

/// Which guarantees a block of this scene provides candidates for.
fn rules_for(scene: SceneId, interaction: Option<Interaction>) -> Vec<MustHave> {
    let mut out = match scene {
        SceneId::FirstLook => vec![MustHave::FirstLook],
        SceneId::CeremonyEntrance => vec![MustHave::CeremonyEntrance],
        SceneId::Vows => vec![MustHave::Vows],
        SceneId::Rings => vec![MustHave::Rings],
        SceneId::Kiss => vec![MustHave::Kiss],
        SceneId::FamilyPortrait => vec![MustHave::FamilyFormals],
        SceneId::ReceptionEntrance => vec![MustHave::ReceptionEntrance],
        SceneId::Cake => vec![MustHave::Cake],
        SceneId::FirstDance => vec![MustHave::FirstDance],
        SceneId::Venue => vec![MustHave::VenueEstablishing],
        SceneId::Exit => vec![MustHave::Exit],
        _ => Vec::new(),
    };
    match interaction {
        Some(Interaction::RingExchange) => out.push(MustHave::Rings),
        Some(Interaction::Kiss) => out.push(MustHave::Kiss),
        Some(Interaction::DanceHold) => out.push(MustHave::FirstDance),
        _ => {}
    }
    out
}
