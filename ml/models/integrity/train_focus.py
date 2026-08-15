"""Train the focus head on labelled sharp/soft/bokeh crops.

PHASE-09 section 6.1 and section 8 step 3, as a plan a reader can argue with and a script
that will run the day a labelled corpus exists.

    python ml/models/integrity/train_focus.py --plan
    python ml/models/integrity/train_focus.py --data focus.npz --out focus_head.pt

## Why there is a `--plan` mode

There is no labelled wedding data in this repository - condition **C1** of
`docs/progress/PHASE-09-EXIT.md` - so this script cannot be run for real here. What it
can do is state the training design precisely enough that the ML lead can disagree with
it *before* the 20,000 crops section 9 gives DATA are labelled against the wrong problem.

`--plan` prints that design, and it is the same shape `ml/models/scene/train_multihead.py
--plan` takes for the same reason.

## The design, in short

**Three classes, not a regression.** The classical measures in
`crates/aura-brain-photo/src/integrity/focus.rs` already produce a sharpness number, and
they produce a good one. What they cannot do is say whether a soft region is a *mistake*:
a veil, a rim light and an f/1.4 background are all low-frequency and only one of them is
a fault. A regressor trained on this data would learn to reproduce the acutance the
analyser already computes - including where it is wrong about the photograph.

**The bokeh class needs deliberate mining.** Random sampling produces almost none of it.
The examples come from wide-aperture frames where the *face* region is sharp and the
background band is not, which is a geometry `RegionSet` already computes - so the miner
is a query over stored verdicts rather than a new pass over pixels.

**Stratify by region kind, not uniformly.** A uniform sample of crops from a wedding is
80 % background band. The four kinds - eye, face, body, background - have wildly
different class balances and the head has to be good on the eye regions specifically,
because that is the region section 6.1 puts first and the one a viewer judges by.

**Wedding-level splits, never frame-level.** Two frames from one burst 300 ms apart are
near-duplicates. A frame-level split puts one in train and one in test and reports an
accuracy that measures leakage.

**Train on the luminance plane at 64 px.** The same single-channel 64 px input the
shipped graph declares. A model trained on colour crops and exported to a one-channel
input is a model whose first convolution has been silently reinterpreted.

## The three numbers a reviewer should push back on

1. **64 px.** Chosen because the head is being asked about the *character* of the edges
   and the classical measures already work at full proxy resolution. If the trained head
   turns out to need finer detail to separate a veil from a soft cheek, this is the
   constant that moves - and it moves `FOCUS_CROP_SIDE`, the graph, and every stored
   `model_ver` with it.
2. **The class weights.** `soft` will be perhaps 15 % of a labelled set and `bokeh`
   perhaps 5 %. Plain cross-entropy on that produces a head that predicts `sharp` for
   everything and is 80 % accurate, which is worse than useless here: `sharp` is the
   class that withdraws no claim.
3. **The asymmetric decision threshold.** `focus::HEAD_OVERRULES_AT` is 0.70 and the head
   is only allowed to *withdraw* a softness claim. After training, lifting that asymmetry
   is an ADR and a re-validation rather than a constant change - and the sweep that
   justifies it should be over false rejections on a held-out wedding, not over accuracy.

## After a successful run

    python ml/models/integrity/export.py --check
    python ml/models/integrity/export.py --focus focus_head.pt --out models/
    cargo xtask models --generate
    cargo xtask models
    cargo test -p aura-brain-photo
"""

from __future__ import annotations

import argparse
import sys

#: The crop side the shipped graph declares. `fixtures::FOCUS_CROP_SIDE`.
CROP_SIDE = 64

#: The three classes, in the head's output order. `FocusClass::ALL`.
CLASSES = ("sharp", "soft", "bokeh")

#: The four region kinds a labelled set must be stratified over.
REGION_KINDS = ("eye", "face", "body", "background")

PLAN = f"""
focus head - training plan
==========================

input      {CROP_SIDE}x{CROP_SIDE} single-channel luminance, 0..1, no mean subtraction
output     {len(CLASSES)}-way softmax in the graph: {", ".join(CLASSES)}
trunk      none. This is a small convolutional head trained from scratch; it does not
           sit on the phase 05 embedding, because sharpness is a local property and the
           embedding is a 384 px global summary that has thrown it away.

data
  source            crops sampled from the catalog at the same scale the analyser uses
  stratify by       region kind: {", ".join(REGION_KINDS)}
  split             by wedding, never by frame
  target size       20,000 crops (section 9, DATA)
  bokeh mining      wide-aperture frames where the face region is sharp and the
                    background band is not; a query over stored verdicts, not a new pass

labels
  sharp   in focus
  soft    softer than it should be - a mistake
  bokeh   deliberately out of focus - not a mistake
  Three sources: focus-bracketed pairs give sharp/soft free, the photographer's own
  keep/reject gives a weak signal, a human labels the ambiguous middle. The middle is
  where the whole value is.

loss      cross-entropy with class weights inverse to frequency
optimiser AdamW, 3e-4, cosine decay, 30 epochs
batch     256
augment   brightness and contrast jitter only. **No blur augmentation** - a blur
          augmentation would relabel a sharp crop as soft without changing its label,
          which is the one augmentation this problem cannot have.

gates     tests/eval/integrity_eval.rs, and section 10.1's AUC >= {0.96}
          measured on held-out *weddings*, per region kind as well as overall
"""


def plan() -> int:
    print(PLAN.strip())
    return 0


def train(data: str, out: str) -> int:
    print(f"would train on {data} and write {out}")
    print(
        "This script has no data to train on in this repository - condition C1 in "
        "docs/progress/PHASE-09-EXIT.md. Run --plan to read the design."
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
    return train(args.data, args.out or "focus_head.pt")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
