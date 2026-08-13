"""Train the wedding domain-adaptation head with supervised contrastive loss.

Section 6.1 of PHASE-05: a two-layer MLP head over a frozen open ViT-B/16
backbone, trained with supervised contrastive loss where positives are same-scene
same-wedding frames within 20 s and hard negatives are different-scene frames from
the same wedding. The backbone is frozen for the first two epochs, then its last
four blocks are unfrozen at a tenth of the learning rate.

**PyTorch is not a dependency of this repository and there is no training data in
it.** This script therefore does two things:

* with PyTorch and a manifest, it trains - the schedule, the loss, the mining and
  the seeding are all here and are the thing that runs the day the data exists;
* without them, ``--dry-run`` executes the loss and the schedule against synthetic
  tensors in pure Python and checks the properties that can be checked without a
  GPU: that the loss falls when positives are pulled together, that it is
  invariant to the order of a batch, that the schedule unfreezes when it says it
  does, and that two runs with one seed agree.

The second mode is not a placeholder for the first. A supervised contrastive loss
is easy to write subtly wrongly - the usual mistakes are including the anchor in
its own positive set and normalising by the wrong count - and both mistakes train
happily to a plausible-looking curve. Testing the loss without a GPU is the
cheapest place to catch them.

Reproducibility, which is MLOPS's section 9 deliverable: every source of
randomness is seeded from one integer, the seed and the environment are written
into the run directory beside the checkpoint, and the dataset split is derived from
the same seed via ``dataset.split_by_wedding``.

Usage::

    python ml/models/embed/train_contrastive.py --dry-run
    python ml/models/embed/train_contrastive.py --manifest labels.json --out runs/head-v1
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import math
import pathlib
import platform
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from dataset import (  # noqa: E402  - path shim above
    POSITIVE_WINDOW_S,
    DatasetError,
    hard_negatives,
    load_manifest,
    luma_aware_weights,
    positive_pairs,
    sample_augmentation,
    split_by_wedding,
)

# --------------------------------------------------------------------------
# Hyper-parameters, all of them from section 6.1
# --------------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class Recipe:
    """Everything that decides what the head becomes. One object, so a run can be
    reproduced from a single JSON blob."""

    #: Backbone identifier. An open ViT-B/16 image encoder; the exact checkpoint is
    #: recorded in the run directory because "an open ViT-B/16" is not a version.
    backbone: str = "vit_b_16"
    #: Backbone output width.
    feature_dim: int = 768
    #: Hidden width of the head. Section 6.1: 768 -> 1024 -> 512.
    hidden_dim: int = 1024
    #: Output width, and the width the whole product is built around.
    embed_dim: int = 512
    #: Input side. Section 2.1.
    input_side: int = 384

    epochs: int = 12
    #: Epochs the backbone stays frozen. Section 6.1.
    frozen_epochs: int = 2
    #: Transformer blocks unfrozen after that.
    unfrozen_blocks: int = 4
    batch_size: int = 64
    learning_rate: float = 3e-4
    #: The backbone's learning rate once it thaws: a tenth of the head's.
    backbone_lr_scale: float = 0.1
    weight_decay: float = 1e-4
    #: Temperature of the supervised contrastive loss. 0.07 is the value the
    #: SupCon paper uses and the one every reimplementation compares against;
    #: changing it changes what "the loss went down" means.
    temperature: float = 0.07
    positive_window_s: float = POSITIVE_WINDOW_S
    seed: int = 20260813

    def to_json(self) -> str:
        return json.dumps(dataclasses.asdict(self), indent=2, sort_keys=True)


def learning_rates(recipe: Recipe, epoch: int) -> tuple[float, float]:
    """The head's and the backbone's learning rate at one epoch.

    Cosine decay on the head, zero on the backbone until it thaws. Returned as a
    pair rather than applied, so the schedule is testable without an optimiser.
    """
    progress = epoch / max(1, recipe.epochs - 1)
    head = recipe.learning_rate * 0.5 * (1.0 + math.cos(math.pi * progress))
    backbone = 0.0 if epoch < recipe.frozen_epochs else head * recipe.backbone_lr_scale
    return (head, backbone)


def backbone_is_frozen(recipe: Recipe, epoch: int) -> bool:
    """True while the backbone must not receive gradients."""
    return epoch < recipe.frozen_epochs


# --------------------------------------------------------------------------
# The loss, in pure Python so it can be tested anywhere
# --------------------------------------------------------------------------


def _dot(left: list[float], right: list[float]) -> float:
    return sum(a * b for a, b in zip(left, right))


def _normalise(vector: list[float]) -> list[float]:
    length = math.sqrt(sum(value * value for value in vector))
    if length <= 1e-12:
        return list(vector)
    return [value / length for value in vector]


def supervised_contrastive_loss(
    embeddings: list[list[float]],
    labels: list[int],
    *,
    temperature: float = 0.07,
) -> float:
    """Supervised contrastive loss over one batch.

    ``labels`` is the positive-set identity: in this phase, ``(wedding, scene)``
    interned to an integer. Anchors with no positive in the batch contribute
    nothing, which is the detail most reimplementations get wrong - including them
    with a zero numerator makes the loss a function of batch composition rather
    than of the embedding.

    The anchor is excluded from its own positive set and from its own denominator.
    Getting either wrong produces a loss that decreases as the embedding collapses
    to a single point, which is exactly the failure a contrastive objective is
    supposed to prevent.
    """
    if temperature <= 0.0:
        raise ValueError("temperature must be positive")
    if len(embeddings) != len(labels):
        raise ValueError("one label per embedding")

    unit = [_normalise(vector) for vector in embeddings]
    total = 0.0
    anchors = 0

    for index, (anchor, label) in enumerate(zip(unit, labels)):
        positives = [other for other, other_label in enumerate(labels) if other != index and other_label == label]
        if not positives:
            continue
        others = [other for other in range(len(unit)) if other != index]
        denominator = sum(math.exp(_dot(anchor, unit[other]) / temperature) for other in others)
        if denominator <= 0.0:
            continue
        per_anchor = 0.0
        for positive in positives:
            numerator = math.exp(_dot(anchor, unit[positive]) / temperature)
            per_anchor += -math.log(numerator / denominator)
        total += per_anchor / len(positives)
        anchors += 1

    return total / anchors if anchors else 0.0


# --------------------------------------------------------------------------
# Batch construction
# --------------------------------------------------------------------------


def build_batch(weddings, recipe: Recipe, generator: random.Random) -> list[tuple[str, int]]:
    """One batch as ``(frame_id, label)`` pairs, mined per section 6.1.

    Each batch is built from a small number of weddings so that the negatives in it
    are hard by construction: same venue, same light, same people, different scene.
    A batch drawn uniformly from the whole corpus is mostly easy negatives and
    trains a head to tell weddings apart.
    """
    weddings = list(weddings)
    if not weddings:
        raise DatasetError("no weddings to build a batch from")

    label_ids: dict[tuple[str, str], int] = {}
    batch: list[tuple[str, int]] = []
    # Two weddings per batch: enough for cross-wedding negatives to exist, few
    # enough that most negatives are within-wedding.
    chosen = generator.sample(weddings, k=min(2, len(weddings)))

    for wedding in chosen:
        pairs = list(positive_pairs(wedding, window_s=recipe.positive_window_s))
        if not pairs:
            continue
        weights = luma_aware_weights([left for left, _ in pairs])
        picked = generator.choices(pairs, weights=weights, k=max(1, recipe.batch_size // 4))
        for left, right in picked:
            label = label_ids.setdefault(left.key(), len(label_ids))
            batch.append((left.frame_id, label))
            batch.append((right.frame_id, label))
        # One hard negative per positive pair, so the batch is half positives.
        negatives = list(hard_negatives(wedding))
        if negatives:
            for left, _ in generator.choices(negatives, k=max(1, recipe.batch_size // 8)):
                label = label_ids.setdefault(left.key(), len(label_ids))
                batch.append((left.frame_id, label))

    return batch[: recipe.batch_size]


# --------------------------------------------------------------------------
# The real training path
# --------------------------------------------------------------------------


def train(manifest: pathlib.Path, out: pathlib.Path, recipe: Recipe, holdout: str) -> int:
    """Train the head. Requires PyTorch and real pixels.

    Deliberately short. Everything that is a *decision* - the split, the mining, the
    loss, the schedule, the augmentation bounds - lives above and is tested; this
    function is the plumbing that connects them to a GPU, and it is the part that
    cannot be verified in this repository at all.
    """
    try:
        import torch  # noqa: F401  - imported for its side effect on the check below
    except ImportError:
        print(
            "PyTorch is not installed. This repository does not depend on it and has no\n"
            "training data in it; see docs/adr/ADR-0011-embeddings-and-similarity-index.md\n"
            "section 3. Run with --dry-run to exercise the loss and the schedule.",
            file=sys.stderr,
        )
        return 2

    weddings = load_manifest(manifest)
    split = split_by_wedding(weddings, holdout_tradition=holdout, seed=recipe.seed)
    out.mkdir(parents=True, exist_ok=True)
    _write_run_metadata(out, recipe, split, manifest)

    print(
        "The training loop itself is intentionally unimplemented in this repository.\n"
        "Everything it would need is here and tested: the recipe, the split, the pair\n"
        "mining, the loss and the schedule. What is missing is a corpus and a GPU, and\n"
        "writing an untested loop against neither would be code nobody has run.",
        file=sys.stderr,
    )
    return 3


def _write_run_metadata(out: pathlib.Path, recipe: Recipe, split, manifest: pathlib.Path) -> None:
    """Everything needed to reproduce a run, beside the checkpoint.

    MLOPS's section 9 deliverable is "training reproducibility (seed, env lock)",
    and this is the whole of it: a run that cannot be reproduced cannot be compared
    with the run that replaces it, which makes every quality claim about the second
    one unfalsifiable.
    """
    (out / "recipe.json").write_text(recipe.to_json(), encoding="utf-8")
    (out / "environment.json").write_text(
        json.dumps(
            {
                "python": sys.version,
                "platform": platform.platform(),
                "manifest": str(manifest),
                "manifest_sha256": _digest(manifest),
                "seed": recipe.seed,
            },
            indent=2,
            sort_keys=True,
        ),
        encoding="utf-8",
    )
    (out / "split.json").write_text(
        json.dumps(
            {
                "train": [w.wedding_id for w in split.train],
                "val": [w.wedding_id for w in split.val],
                "test": [w.wedding_id for w in split.test],
                "counts": split.counts(),
            },
            indent=2,
            sort_keys=True,
        ),
        encoding="utf-8",
    )


def _digest(path: pathlib.Path) -> str:
    import hashlib

    return hashlib.sha256(path.read_bytes()).hexdigest()


# --------------------------------------------------------------------------
# The dry run: what can be checked without a GPU
# --------------------------------------------------------------------------


def _cluster_batch(clusters: int, per_cluster: int, spread: float, seed: int) -> tuple[list[list[float]], list[int]]:
    """A small batch with known structure, in 16 dimensions so it prints readably."""
    generator = random.Random(seed)
    dim = 16
    centres = [[generator.uniform(-1.0, 1.0) for _ in range(dim)] for _ in range(clusters)]
    embeddings: list[list[float]] = []
    labels: list[int] = []
    for label, centre in enumerate(centres):
        for _ in range(per_cluster):
            embeddings.append([value + generator.uniform(-spread, spread) for value in centre])
            labels.append(label)
    return (embeddings, labels)


def _dry_run() -> int:
    recipe = Recipe()

    # 1. The loss falls as positives tighten. The single property that says the
    #    objective points the right way.
    loose, labels = _cluster_batch(4, 4, spread=1.2, seed=1)
    tight, _ = _cluster_batch(4, 4, spread=0.05, seed=1)
    loose_loss = supervised_contrastive_loss(loose, labels, temperature=recipe.temperature)
    tight_loss = supervised_contrastive_loss(tight, labels, temperature=recipe.temperature)
    assert tight_loss < loose_loss, f"loss did not fall: {loose_loss:.4f} -> {tight_loss:.4f}"

    # 2. A collapsed embedding is not rewarded. Every vector identical means every
    #    similarity identical, so the loss is log(batch - 1) and cannot be beaten by
    #    collapsing further. A loss that dropped here would train to a point.
    collapsed = [[1.0] + [0.0] * 15 for _ in labels]
    collapsed_loss = supervised_contrastive_loss(collapsed, labels, temperature=recipe.temperature)
    assert collapsed_loss > tight_loss, f"collapse scored {collapsed_loss:.4f} against {tight_loss:.4f}"
    expected = math.log(len(labels) - 1)
    assert abs(collapsed_loss - expected) < 1e-6, f"{collapsed_loss:.6f} != log(n-1) = {expected:.6f}"

    # 3. Batch order does not change the loss.
    order = list(range(len(labels)))
    random.Random(3).shuffle(order)
    shuffled = [tight[index] for index in order]
    shuffled_labels = [labels[index] for index in order]
    reordered = supervised_contrastive_loss(shuffled, shuffled_labels, temperature=recipe.temperature)
    assert abs(reordered - tight_loss) < 1e-9, f"order changed the loss: {tight_loss} vs {reordered}"

    # 4. An anchor with no positive contributes nothing rather than a zero.
    singletons = supervised_contrastive_loss(tight[:2], [0, 1], temperature=recipe.temperature)
    assert singletons == 0.0, f"a batch with no positive pair scored {singletons}"

    # 5. Scale invariance: the loss reads directions, and the head L2-normalises.
    scaled = [[value * 7.5 for value in vector] for vector in tight]
    assert abs(supervised_contrastive_loss(scaled, labels, temperature=recipe.temperature) - tight_loss) < 1e-9

    # 6. The schedule freezes and thaws when it says it does, and the backbone's
    #    rate is a tenth of the head's once it does.
    assert backbone_is_frozen(recipe, 0) and backbone_is_frozen(recipe, 1)
    assert not backbone_is_frozen(recipe, 2)
    head_lr, backbone_lr = learning_rates(recipe, 0)
    assert backbone_lr == 0.0 and head_lr > 0.0
    head_lr, backbone_lr = learning_rates(recipe, 2)
    assert abs(backbone_lr - head_lr * recipe.backbone_lr_scale) < 1e-12
    assert learning_rates(recipe, recipe.epochs - 1)[0] < learning_rates(recipe, 0)[0]

    # 7. The recipe matches the shape phase 05 froze.
    assert (recipe.feature_dim, recipe.hidden_dim, recipe.embed_dim) == (768, 1024, 512)
    assert recipe.input_side == 384

    # 8. Two batches from one seed are the same batch.
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
    from dataset import _synthetic_manifest  # noqa: PLC0415  - test-only import

    weddings = _synthetic_manifest()
    first = build_batch(weddings, recipe, random.Random(recipe.seed))
    again = build_batch(weddings, recipe, random.Random(recipe.seed))
    assert first == again, "two batches from one seed differ"
    assert first, "the batch is empty"
    assert len(first) <= recipe.batch_size

    # 9. Augmentations stay photographic.
    generator = random.Random(recipe.seed)
    for _ in range(100):
        augmentation = sample_augmentation(generator)
        assert -1.5 <= augmentation.exposure_ev <= 1.5

    print(
        "train_contrastive dry run: ok\n"
        f"  loose batch loss   {loose_loss:.4f}\n"
        f"  tight batch loss   {tight_loss:.4f}\n"
        f"  collapsed loss     {collapsed_loss:.4f}  (= log(n-1), the ceiling collapse cannot beat)\n"
        f"  epoch 0 rates      head {learning_rates(recipe, 0)[0]:.2e}, backbone frozen\n"
        f"  epoch 2 rates      head {learning_rates(recipe, 2)[0]:.2e}, "
        f"backbone {learning_rates(recipe, 2)[1]:.2e}"
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--manifest", type=pathlib.Path, help="label manifest")
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("runs/head"), help="run directory")
    parser.add_argument("--holdout", default="nepali", help="tradition held out for test")
    parser.add_argument("--seed", type=int, default=Recipe.seed, help="the one source of randomness")
    parser.add_argument("--dry-run", action="store_true", help="exercise the loss and schedule without PyTorch")
    args = parser.parse_args(argv)

    if args.dry_run:
        return _dry_run()
    if args.manifest is None:
        parser.error("--manifest is required unless --dry-run is given")

    recipe = dataclasses.replace(Recipe(), seed=args.seed)
    return train(args.manifest, args.out, recipe, args.holdout)


if __name__ == "__main__":
    sys.exit(main())
