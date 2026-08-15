"""Train the eye-state head on consented, aligned face crops.

PHASE-09 section 6.4 and section 8 step 7, as a plan a reader can argue with and a script
that will run the day a consented corpus exists.

    python ml/models/integrity/train_eyes.py --plan
    python ml/models/integrity/train_eyes.py --data eyes.npz --out eye_state.pt

## Why there is a `--plan` mode, and why this one is different from the others

There is no consented face data in this repository - phase 06's condition **C1**,
unchanged - so this script cannot be run for real here.

But the reason is stronger than "the data has not been collected". **An eye-state dataset
is more sensitive than a detection dataset, not less.** A labelled set of closed-eye
photographs of identifiable people at private events is precisely the artefact
`docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` and
`docs/pdf/24-PRIVACY-CONSENT-AND-LEGAL` exist to govern, and building it is a consent
question before it is a machine-learning question.

So `--plan` prints the design *and* the consent preconditions, and the second list is
not advisory.

## The design, in short

**Five classes, and slots 3 and 4 are why this is a model.** Open against closed is
nearly separable with an eyelid-aperture heuristic. Open against *looking down* is not,
and a bride looking down at her bouquet is a photograph while a bride mid-blink is a
reshoot. `occluded` is the veil case, and reading it as `closed` turns every veiled face
in a Hindu ceremony into a defect.

**Label the state and the intent separately.** Section 8 step 7 says "label eye states
including intentional-closed cases", and they are two labels rather than one: a frame is
`closed` *and* `intentional`. Collapsing them into a four-class problem would train the
head to predict intent from a crop - and intent depends on the scene, the partner and the
expression, none of which is in the crop. The intent labels train nothing here; they are
the test set for `eyes::intent`, which is rules a photographer signed off.

**The class balance is brutal.** `open` is 90 %+ of any real sample. Class-balanced
sampling, and evaluate with F1 rather than accuracy - which section 10.1 already asks for
and which is the difference between a useful head and one that is 91 % accurate by never
predicting a blink.

**`looking_down` needs deliberate mining**, from `getting_ready`, `details` and
`speeches`, where people look at what they are holding.

**Train on the two-point aligned crop the analyser produces**, not on phase 06's
five-point one. They are different crops: the recogniser's warp uses the nose and mouth
corners and this one does not, because `FaceRef` deliberately does not carry them
(ADR-0019 section 3). A head trained on the recogniser's crop and run on this one is a
head being asked about slightly different pixels than it learned.

## The consent preconditions, which are not advisory

1. Every crop comes through `PeopleService`'s sealed store, under the project's recorded
   consent, and no crop is written outside it.
2. The labelled set is per-project and is deleted with the project. There is no corpus
   that outlives the weddings it came from.
3. Demographic labels are not collected. Section 11 of the operating manual - "never
   infer anything about a person" - is why, and the fairness analysis in
   `docs/model-cards/eye_state.md` is written against groups agreed with PM rather than
   against attributes this product stores.
4. The fairness measurement is a *precondition of publishing a number*, not a follow-up.
   An eye-state head that is worse at "open" for one group would systematically mark that
   group's photographs as defective, and that is the most direct route from this product
   to a discriminatory outcome.

## After a successful run

    python ml/models/integrity/export.py --check
    python ml/models/integrity/export.py --eyes eye_state.pt --out models/
    cargo xtask models --generate
    cargo xtask models
    cargo test -p aura-brain-photo
"""

from __future__ import annotations

import argparse
import sys

#: The crop side the shipped graph declares. `fixtures::EYE_CROP_SIDE`.
CROP_SIDE = 112

#: The five classes, in the head's output order. `EyeOpenness::ALL`.
CLASSES = ("open", "squint", "closed", "looking_down", "occluded")

PLAN = f"""
eye-state head - training plan
==============================

input      {CROP_SIDE}x{CROP_SIDE} three-channel sRGB, 0..1, two-point eye alignment
output     {len(CLASSES)}-way softmax in the graph: {", ".join(CLASSES)}
           **No intent slot.** Intent depends on the scene, the partner and the
           expression, none of which is in the crop.

consent preconditions (not advisory - see this file's header)
  1. crops come through PeopleService's sealed store, under recorded consent
  2. the labelled set is per-project and dies with the project
  3. no demographic labels are collected
  4. the fairness measurement is a precondition of publishing any number

data
  alignment   two-point similarity onto the ArcFace canonical eye positions, which is
              what `eyes::align_from_eyes` produces. NOT phase 06's five-point warp.
  split       by wedding, never by frame
  sampling    class-balanced; `open` is 90%+ of any real sample
  mining      `looking_down` from getting_ready, details and speeches

labels
  state       one of the five above
  intent      a separate boolean on closed examples. Trains nothing; it is the test set
              for `eyes::intent`, which is rules rather than a model.

loss        cross-entropy, class-balanced sampling rather than class weights - weights on
            a 90/10 split make the loss surface unstable long before they fix the balance
optimiser   AdamW, 3e-4, cosine decay, 40 epochs
batch       128
augment     brightness, contrast, small rotation. **No horizontal flip on `looking_down`**
            - the class is about gaze direction and a flip does not preserve it.

gates       tests/eval/integrity_eval.rs, and section 10.1's F1 >= 0.95 with an
            intentional-closed false-positive rate <= 2%, measured on held-out weddings
            AND per group, before any number is published
"""


def plan() -> int:
    print(PLAN.strip())
    return 0


def train(data: str, out: str) -> int:
    print(f"would train on {data} and write {out}")
    print(
        "This script has no consented data to train on in this repository - phase 06's "
        "condition C1 and phase 09's. Run --plan to read the design and the consent "
        "preconditions."
    )
    return 1


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--plan", action="store_true", help="print the training design")
    parser.add_argument("--data", help="npz of crops and labels")
    parser.add_argument("--out", help="where to write the trained head")
    args = parser.parse_args(argv)

    if args.plan or args.data is None:
        return plan()
    return train(args.data, args.out or "eye_state.pt")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
