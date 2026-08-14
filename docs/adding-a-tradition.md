# Adding a wedding tradition

This is the procedure section 2.1 means by "an extensible taxonomy file", and section
12's first failure mode - cultural blind spots - is the reason it exists. A tradition
this product has never heard of must be addable by editing a file, and this page is what
that looks like end to end.

**Who this is for.** A wedding photographer, a cultural consultant, or an engineer
working with one. Steps 1 to 5 need a text editor. Step 6 needs a training run and is
the only part that needs an ML engineer.

---

## 1. Decide whether you are adding a tradition or a rite

Most of the time you are adding a **rite to an existing file**, and that is much easier.

Add a rite to an existing tradition when the wedding is one the shipped files already
cover - Hindu, Nepali, Christian, Muslim, civil - and it does something they do not name.
A Gujarati wedding is a Hindu wedding with rites `hindu.toml` is missing, not a sixth
tradition.

Add a **new tradition file** when the day has a different *structure*: a different order,
a different set of participants, a different name for the whole event. Sikh, Jewish,
Buddhist and Yoruba weddings are traditions. A regional variation of a rite is not.

The cost is very different, and it is worth knowing before you start:

| | Effort | Needs a retrain |
|---|---|---|
| A rite in an existing file | ten minutes | no, if there is a free id in that tradition's range |
| A new tradition file, ids under 160 | an hour | no, but the head cannot name the new rites until it is trained on them |
| A new tradition past id 159 | a week | **yes**, and the head's width changes |

---

## 2. Find a free id

Open `crates/aura-brain-wedding/config/rituals/` and read the header of `hindu.toml`.
It carries the reserved ranges:

```
hindu      10 -  39
nepali     40 -  69
christian  70 -  99
muslim    100 - 129
civil     130 - 159
```

**The id is the model's output slot.** That is the one rule everything else follows
from:

- an id is **never reused**, because reusing one relabels a trained output;
- an id is **never derived from file order**, because inserting a rite would renumber
  every rite after it and silently relabel every stored row in every catalog;
- ids are **unique across every file**, because two rites cannot share a logit.

`Taxonomy::load` refuses all three with `AURA-ML-5024`, and
`crates/aura-brain-wedding/tests/config.rs` asserts the shipped files satisfy them.

For a rite in an existing tradition, take the next free number in that tradition's range.
For a new tradition, claim the next free block of thirty and write it into `hindu.toml`'s
header table so the next person can see it is taken.

---

## 3. Write the entry

```toml
[[ritual]]
id          = 25
slug        = "anand_karaj"
title       = "Anand Karaj"
aliases     = ["lavan", "four rounds"]
cloud_label = "saptapadi_pheras"
phase       = "day"
night       = false
evidence    = "the couple circling the Guru Granth Sahib, a palki, a ragi group to one side"
```

Field by field:

| Field | Rule |
|---|---|
| `id` | See step 2. Required. |
| `slug` | Lower-case ASCII, digits and underscores only. This goes into the catalog and is stable for ever. |
| `title` | What a photographer sees. Required - a rite with no title shows a slug. |
| `aliases` | Regional and transliterated spellings. Matched case- and punctuation-insensitively, never stored. Free to add generously. |
| `cloud_label` | Optional. A member of `aura_cloud::tasks::ALLOWED_RITUALS` when phase 04's frozen vocabulary has a word for this rite. Omit it when it does not; that vocabulary is frozen into recorded cassettes and every cached answer, so it is mapped rather than edited. |
| `phase` | `pre`, `day` or `post`. |
| `night` | Whether the rite is typically after dark. Read by nothing at run time; it is the evidence behind the ritual scene's noise tolerance in `scene_profiles.toml`. |
| `evidence` | **The most valuable field in the file.** Visible objects and staging a person or a classifier can key on. This is the knowledge a consultant has and a file does not, and it is what a labelling brief is written from. |

A whole new tradition file starts with a header:

```toml
version   = 1
tradition = "sikh"
title     = "Sikh"
```

and `tradition` must be one of the eight in `aura_brain_wedding::scene::ritual::TRADITIONS`.
Adding a ninth is a change to the ritual head's input width, which is a retrain.

**Declare a rite once.** If a rite already exists in another file - a Sikh wedding also
has a `mehendi` - do not redeclare it. The five files are loaded as one table and every
tradition matches every rite through the union. `nepali.toml`'s header explains this at
length, because it is the file where the temptation is strongest.

---

## 4. Check it

```bash
cargo test -p aura-brain-wedding --test config
```

That runs the same loader the product runs, over the shipped files. It fails with the
file, the key and the rule when anything is wrong. Read the message: it is written to be
the answer rather than the start of an investigation.

The common failures, in the order they actually occur:

| Message contains | What to do |
|---|---|
| `reuses id N` | Another file already uses that number. Take the next free one in your range. |
| `is not snake_case ascii` | The slug is a catalog value and must survive any locale. |
| `has no title` | A photographer would see a slug. |
| `at or above the ritual head's 160 output slots` | You have run out of range. See step 6. |
| `is already declared` | Declare a rite once; see step 3. |

---

## 5. Add the profile, if the tradition needs different tolerances

Usually it does not. `scene_profiles.toml` is keyed by **scene**, not by tradition, and a
`ritual` scene is a `ritual` scene whether it is a saptapadi or an anand karaj.

The one case where it does: if the new tradition's central rites happen in light the
shipped `ritual` profile does not anticipate. Read that profile's rationale first -

> The highest noise tolerance of any non-dance scene, and the reason is specific: Hindu,
> Nepali and Sikh rites are frequently held at night around fire, indoors, under a
> mandap, with no flash permitted.

- and if your tradition is already covered by that argument, change nothing.

If it is not, do **not** loosen the shipped `ritual` profile to accommodate one tradition;
that would relax the tolerance for every wedding. Raise it with PM. Section 8 step 7 of
the phase document makes `scene_profiles.toml` a product decision with a named owner, and
every value in it needs a rationale a photographer would agree with.

---

## 6. Train the head, which is the part that needs an ML engineer

Steps 1 to 5 make the rite **nameable**: it appears in the taxonomy, a photographer can
search for it, a cloud `SegmentNaming` answer can map to it, and it can be set by hand.

They do not make it **detectable**. The ritual head emits a fixed 160 logits and it has
only ever been trained on the rites that existed when it was trained. A new rite's slot
is dead until a training run includes it.

```bash
python ml/models/scene/train_ritual.py --plan
```

Read that plan before collecting anything. The two sentences that matter most:

> Oversample any rite below 60 examples to the floor. **Below about twenty examples a
> rite should be REMOVED from the training set and left to abstention** - a rite the head
> has seen twelve times will produce confident wrong answers on unrelated frames, which
> is worse than silence.

and

> A religious ceremony is sensitive material. The dataset record states who agreed to
> what, per wedding and per tradition, before any frame enters the set.

**Past id 159** the head's width changes, which means `RITUAL_SLOTS` in
`aura_infer::onnx::fixtures`, a re-export, a re-sign, a model card update and a
`MODEL_VER` bump that re-reads every stored ritual slug. `Taxonomy::load` refuses the id
rather than letting that happen by accident.

---

## 7. What good looks like

A tradition is properly added when all five are true:

1. `cargo test -p aura-brain-wedding` is green.
2. A photographer can find the rite by any spelling they would use.
3. `docs/model-cards/ritual_classifier.md`'s fairness section names the new tradition in
   its per-tradition table - **reported, even before it is gated**.
4. The head either names the rite reliably or **abstains on it**, and the eval reports
   which. A head that abstains is doing its job; a head that guesses is the failure this
   whole design is arranged against.
5. Nobody has loosened a shipped threshold to make one tradition pass.

---

## Related

- `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md` section 3
- `crates/aura-brain-wedding/config/rituals/hindu.toml` - the reference file, with the
  full field documentation in its header
- `docs/model-cards/ritual_classifier.md` - what the head can and cannot do
- `docs/runbooks/AURA-ML-5024.md` - every refusal the loaders make
