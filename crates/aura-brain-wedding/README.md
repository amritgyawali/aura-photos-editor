# `aura-brain-wedding`

Scenes, rituals and the wedding's ordered story. The scene graph every later threshold is
conditioned on.

## What this crate is for, in one paragraph

Invariant 7 says *"no threshold is global; every threshold is a function of the detected
scene and subject role."* For six phases that was a promise with nothing behind it,
because there was no scene. This crate produces one, and `SceneProfile` is the lookup that
turns the promise into a join. A dark dance frame and a formal family portrait are judged
by different rows of the same table from here onward.

## The two halves

**[`scene`]** answers *what is this photograph of*. One of 22 classes, fourteen attribute
bits, and a named rite when the evidence supports one.

**[`story`]** answers *where in the day is this*. Per-frame posteriors are smoothed by a
hidden Markov model over nine chapters, a change-point detector finds the boundaries, and
a medoid picks each chapter's cover.

## Five things worth knowing before you change anything here

### 1. Nothing in this crate opens a pixel

The classifier is an **adapter on the frozen phase 05 embedding** - section 6.1's design.
It takes a 512-d vector that was computed once, in phase 05, plus sixteen numbers the
catalog already holds. That is why section 11 budgets 35 seconds for four thousand images
where phase 06's face pass needs twelve minutes for the same wedding: the expensive part
already happened.

A change that made this crate decode an image would be a change to the phase's cost model,
not an optimisation.

### 2. The abstention is in the decoder, not in the graph

`SceneId::Unknown` is not a softmax slot, and `RitualId::NONE` **is** one. That asymmetry
is deliberate and it is the most likely thing to be "fixed" by mistake.

A model cannot usefully be trained to say "I am not sure" through an output that competes
with the classes it is unsure between: the gradient that teaches it to pick `unknown` is
the same gradient that teaches it not to pick `ceremony`. So the scene head emits 22
classes and `classifier::decode_scene` decides, on a **margin** rather than on a floor.

"No rite" is different. It is a real, positive, learnable state of the world - a ceremony
frame with no named rite in it - so it competes fairly in the ritual softmax at slot 0.
The ritual head *also* has a margin, and it handles a third case: a head that has
correctly identified a fire circumambulation and cannot tell whether to call it
`saptapadi_pheras` or `saat_phera`.

### 3. Smooth first, segment second

ADR-0015 section 6 settles the order the phase document's diagram draws ambiguously.

A single misclassified frame in the reception is a wrong label. Fed to a change-point
detector it is a **chapter boundary**, and the photographer gets a two-frame "Getting
Ready" chapter between the speeches and the cake. By then no amount of smoothing helps,
because the label is a row in `segments`.

### 4. The penalty is searched, not fixed

A PELT penalty tuned on a ten-hour Hindu wedding gives two chapters for a ninety-minute
registry office and forty for a three-day Nepali wedding. Section 10.1 requires *every*
wedding to land between 6 and 20 chapters, so the penalty is a free parameter the
segmenter solves for.

**The search is logarithmic.** A penalty is a scale parameter; linear bisection of
`0.0005..40` spends its first ten steps between 40 and 0.04 and never reaches the bottom
two decades. One of the shipped fixtures answers at 0.008.

### 5. The config files refuse things, and that is the design

`SceneProfileRegistry::load` refuses a profile without a rationale. `Taxonomy::load`
refuses a duplicate ritual id. Both halt with `AURA-ML-5024`, and both are the only things
in phase 07 that stop anything.

Every other failure here degrades into a wedding that is still usable - a frame with no
scene, a chapter with no profile, a timeline segmented by time gaps alone. A half-loaded
threshold table is different in kind: it silently changes every downstream number, which
is exactly the class of failure invariant 9 forbids.

**A ritual id is the model's output slot.** That is why a duplicate is refused rather than
resolved: reusing one relabels a trained output.

## Layout

```
src/
  scene/
    taxonomy.rs    the five ritual files, loaded as one table
    profile.rs     the threshold registry, and the rationale rule
    attributes.rs  sixteen context features in, fourteen bits out
    classifier.rs  the scene head and the abstention rules
    ritual.rs      the rite head, the mask and the two abstentions
    pass.rs        the resumable project walk
  story/
    hmm.rs         Viterbi over nine chapters
    changepoint.rs PELT, the fused signal and the penalty search
    keyframe.rs    medoid, with a three-step relaxation
    segment.rs     the four tables, and the user-override guards
    naming.rs      the SegmentNaming cost policy
    api.rs         `Story`, the one StoryService
  fixtures.rs      three synthetic weddings with known answers
  errors.rs        AURA-ML-5022 to 5027
config/
  scene_profiles.toml
  rituals/{hindu,nepali,christian,muslim,civil}.toml
```

## Running the gates

```bash
cargo test -p aura-brain-wedding          # 45 tests: config rules and section 10.1
cargo run -p aura-cli -- verify --phase 07 --work target/phase07-verify
cargo test --release -p aura-perf --test scene_budgets -- --test-threads=1
```

## The models are placeholders

`scene_classifier` and `ritual_classifier` 1.0.0 have the right architecture and none of
the training. **Condition C1 of `docs/progress/PHASE-07-EXIT.md`, a Sev 2 trigger.** Every
number the gates produce is a measurement of the algorithms against synthetic ground
truth, not of the weights. `tests/eval/scene_eval.rs::the_gate_rejects_a_useless_classifier`
exists so that this is checkable rather than asserted.

## Related

- `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md`
- `docs/adr/ADR-0016-story-ipc-surface.md`
- `docs/adding-a-tradition.md`
- `docs/model-cards/{scene_classifier,ritual_classifier}.md`
