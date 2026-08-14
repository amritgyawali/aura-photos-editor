"""Train the tradition-conditioned ritual head, with abstention.

PHASE-07 section 6.1: "a tradition-conditioned ritual head that is only evaluated when
the segment context suggests a ceremony", with "rare rituals get oversampling plus a
'don't guess' abstain threshold".

    python ml/models/scene/train_ritual.py --plan
    python ml/models/scene/train_ritual.py --data rituals.npz --out ritual_head.pt

## Why this is the harder of the two heads, and the plan says so

The scene head has twenty-two roughly-populated classes. This one has forty-eight rites
across five traditions, several of which appear in one wedding in fifty, and the labels
need a cultural consultant rather than an annotator. Three consequences shape the design:

**The target for a rare rite is abstention, not accuracy.** `griha_pravesh` will have
twelve examples. A head that names it correctly nine times out of twelve and names it
wrongly on forty unrelated frames is worse than one that abstains on all of them, and an
F1 averaged over rites hides that. So the eval reports **abstention rate per rite**
beside F1, and the training plan optimises for a calibrated posterior rather than for
argmax accuracy.

**The slot is the id.** `RITUAL_SLOTS` is 160 and the slot a rite occupies is the `id`
authored in its taxonomy file. Renumbering a rite relabels a trained output, which is why
ADR-0015 forbids deriving ids from file order and why `Taxonomy::load` refuses a
duplicate. The training set's label column is an id, not a name.

**Slot 0 is a class, not a threshold.** "No rite" competes in the same softmax as every
rite. A head that believes there is no rite says so through the distribution, which is
well-posed; the `ABSTAIN_MARGIN` in the decoder handles the *different* case of a head
that cannot choose between two traditions' names for the same rite.

## What this script cannot do today

The head is a placeholder - condition **C1** of `docs/progress/PHASE-07-EXIT.md` - and
there is no labelled ritual corpus. `--plan` is the deliverable.
"""

from __future__ import annotations

import argparse
import sys
from typing import Sequence

RITUAL_SLOTS = 160
TRADITIONS = ("hindu", "nepali", "muslim", "christian", "sikh", "civil", "mixed", "unclear")
ADAPTER_DIM = 192
EMBED_DIM = 512
CONTEXT_SLOTS = 16
FOCAL_GAMMA = 2.5
OVERSAMPLE_FLOOR = 60
EPOCHS = 60
BATCH = 128

PLAN = f"""
ritual_classifier training plan - PHASE-07 section 6.1
======================================================

INPUT
  features [N, {EMBED_DIM + CONTEXT_SLOTS + len(TRADITIONS)}]
    frozen trunk ({EMBED_DIM}) + context ({CONTEXT_SLOTS}) + tradition one-hot ({len(TRADITIONS)})
  labels   [N]  a RITUAL ID from a taxonomy file, 0 = no rite

ARCHITECTURE
  Gemm {ADAPTER_DIM}x{EMBED_DIM + CONTEXT_SLOTS + len(TRADITIONS)} -> ReLU
    -> Gemm {RITUAL_SLOTS}x{ADAPTER_DIM} -> Softmax (in the exported graph)

  {RITUAL_SLOTS} slots because the reserved id ranges run to 159. Slot 0 is
  RitualId::NONE. A sixth tradition needs a wider head, which is a retrain, which
  is why Taxonomy::load refuses an id at or above {RITUAL_SLOTS}.

TRAINING SET
  ONLY ceremony-context frames. The head is never run on a detail shot at
  inference (ritual::should_evaluate), so training it on one teaches it to
  produce an answer for a photograph it will never see.

  Negative examples are ceremony frames with NO named rite, labelled slot 0.
  They must be roughly a third of the set: a head trained only on rites learns
  that every ceremony frame is a rite.

LOSS
  Focal, gamma = {FOCAL_GAMMA}, alpha = inverse frequency, over all {RITUAL_SLOTS} slots.
  Gamma above the scene head's {2.0} because the imbalance is worse: the rarest
  rites have tens of examples against thousands of slot-0 negatives.

SAMPLING
  Oversample any rite below {OVERSAMPLE_FLOOR} examples to the floor. Below about
  twenty examples a rite should be REMOVED from the training set and left to
  abstention - a rite the head has seen twelve times will produce confident wrong
  answers on unrelated frames, which is worse than silence.

CONDITIONING
  The tradition one-hot is present at train time AND the mask is applied at decode
  time (Taxonomy::mask_for, with renormalisation). Both, because they do different
  work: the one-hot stops the head spending capacity separating a saptapadi from a
  communion, and the mask makes a communion on a Hindu wedding impossible rather
  than merely unlikely.

  During training the one-hot is dropped out 20 % of the time, so the head still
  works on the first frames of a wedding before any tradition is established.

ABSTENTION
  Slot 0 is a class and is trained. The margin (ritual::ABSTAIN_MARGIN, 0.10) is a
  decoder rule and is NOT trained: it handles the case where the head has correctly
  identified a fire circumambulation and cannot tell whether to call it
  `saptapadi_pheras` or `saat_phera`, which is a different failure from "no rite".

WHAT THE EVAL MUST REPORT, AND IN THIS ORDER
  1. abstention rate per rite      <- the number that matters for rare rites
  2. F1 per tradition              <- section 10.1's gate, >= 0.85
  3. F1 per rite                   <- where a tradition's average is hiding a hole
  4. confusion between traditions  <- the failure this product exists to avoid

  A head that abstains on every Nepali rite and names every Christian one has a
  fine average F1 and is useless to half the customers this product is built for.
  That is why (1) and (4) come before the gate.

CONSENT
  A religious ceremony is sensitive material. The dataset record states who agreed
  to what, per wedding and per tradition, before any frame enters the set.
"""


def main(argv: Sequence[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true")
    parser.add_argument("--data")
    parser.add_argument("--out", default="ritual_head.pt")
    args = parser.parse_args(argv)

    if args.plan or not args.data:
        print(PLAN.strip())
        if not args.plan:
            print("\n(no --data given, so the plan is what you get)")
        return 0

    print(
        "There is no labelled ritual corpus in this repository - condition C1 in "
        "docs/progress/PHASE-07-EXIT.md. Run --plan.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
