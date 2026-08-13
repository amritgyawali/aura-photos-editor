"""The wedding embedding dataset: splits, pair mining and the augmentation policy.

This file is the specification of the training data, written as code so that the
rules cannot be paraphrased away later. It runs today - the split logic, the
positive and negative mining and the augmentation policy are all exercised by
``python -m pytest ml/models/embed`` or by running this file directly - against a
manifest of *labels*, not of pixels. The pixels do not exist in this repository.

Three rules here are not preferences and are enforced rather than documented.

**Splits are by wedding, never by frame.** Two frames from the same ceremony,
seconds apart, are nearly the same photograph. A frame-level split puts one in
train and one in validation and reports a retrieval score that is memorisation.
:func:`split_by_wedding` refuses to emit a split where a wedding id appears in
two folds.

**One tradition is held out entirely.** Section 12 of the phase document names
"domain head overfits to fixture weddings" as the failure mode, with
"cross-tradition holdout" as the mitigation. A head that learned to recognise one
tradition's palette and calls that scene understanding passes an ordinary
validation split. :func:`split_by_wedding` therefore takes ``holdout_tradition``
and puts every wedding of that tradition in the test fold and nowhere else.

**Augmentations are photographic, never geometric.** Section 6.1: exposure
+/- 1.5 EV, white balance +/- 800 K, mild noise, JPEG artefacts. No flips of
ritual scenes, because handedness carries meaning in the rituals this product is
built for - which hand holds the thread, which shoulder the sari crosses, which
way the couple circles the fire. A flipped ritual frame is not an augmented
example of that ritual; it is an example of something that did not happen. No
heavy crops either: composition is what phase 11 scores, and teaching the
embedding that a third of the frame is optional teaches it to ignore composition.

Usage::

    python ml/models/embed/dataset.py --manifest path/to/labels.json --summary
    python ml/models/embed/dataset.py --self-test
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import pathlib
import random
import sys
from typing import Iterable, Iterator

# --------------------------------------------------------------------------
# The label schema
# --------------------------------------------------------------------------

#: Seconds within which two same-scene frames of the same wedding are positives.
#: Section 6.1. Twenty seconds is about the length of one ritual beat: long enough
#: that a pair is not just two frames of one burst, short enough that the two are
#: reliably the same moment.
POSITIVE_WINDOW_S = 20.0

#: Scene vocabulary. Deliberately small and deliberately not a taxonomy: these are
#: the scenes a photographer names when culling, and phase 07 owns any expansion.
SCENES = (
    "preparation",
    "portraits",
    "ceremony",
    "ritual",
    "group",
    "reception",
    "dance",
    "details",
    "candid",
    "other",
)

#: Traditions, for the cross-tradition holdout. Not an exhaustive list of the
#: world's weddings - a list of what the corpus contains, which is what a holdout
#: can be drawn from.
TRADITIONS = ("hindu", "christian", "nepali", "sikh", "muslim", "civil", "other")


@dataclasses.dataclass(frozen=True)
class Frame:
    """One labelled frame. No pixels: a path and what a human said about it."""

    frame_id: str
    wedding_id: str
    path: str
    #: Clock-aligned capture time in seconds since the epoch. The same timeline
    #: the catalog computes, because a positive pair is defined by it.
    timeline_s: float
    scene: str
    #: Ritual label inside a scene, when the annotator gave one. Free text: the
    #: rituals worth naming differ by tradition and a fixed enum would flatten
    #: exactly the distinctions the head is meant to learn.
    ritual: str | None = None
    camera_id: str | None = None
    #: Set by the annotator when this frame is a duplicate of another. The
    #: duplicate ground truth section 6.4's gate is measured against.
    duplicate_of: str | None = None

    def key(self) -> tuple[str, str]:
        """Scene identity within a wedding: the unit a positive pair shares."""
        return (self.wedding_id, self.scene if self.ritual is None else f"{self.scene}/{self.ritual}")


@dataclasses.dataclass(frozen=True)
class Wedding:
    """One wedding: the unit a split may divide on."""

    wedding_id: str
    tradition: str
    frames: tuple[Frame, ...]

    @property
    def scenes(self) -> set[str]:
        return {frame.scene for frame in self.frames}


@dataclasses.dataclass(frozen=True)
class Split:
    """A wedding-level split."""

    train: tuple[Wedding, ...]
    val: tuple[Wedding, ...]
    test: tuple[Wedding, ...]

    def counts(self) -> dict[str, int]:
        return {
            "train_weddings": len(self.train),
            "val_weddings": len(self.val),
            "test_weddings": len(self.test),
            "train_frames": sum(len(w.frames) for w in self.train),
            "val_frames": sum(len(w.frames) for w in self.val),
            "test_frames": sum(len(w.frames) for w in self.test),
        }


class DatasetError(RuntimeError):
    """A refusal. Every one of these is a rule from the phase document."""


# --------------------------------------------------------------------------
# Loading
# --------------------------------------------------------------------------


def load_manifest(path: pathlib.Path) -> tuple[Wedding, ...]:
    """Read a label manifest and validate every rule that can be checked statically.

    The manifest is JSON: ``{"weddings": [{"wedding_id", "tradition", "frames": [...]}]}``.
    """
    payload = json.loads(path.read_text(encoding="utf-8"))
    weddings: list[Wedding] = []
    seen_frames: set[str] = set()

    for entry in payload.get("weddings", []):
        tradition = entry.get("tradition", "other")
        if tradition not in TRADITIONS:
            raise DatasetError(f"unknown tradition {tradition!r}; add it to TRADITIONS first")
        frames: list[Frame] = []
        for raw in entry.get("frames", []):
            scene = raw.get("scene")
            if scene not in SCENES:
                raise DatasetError(f"unknown scene {scene!r} in {entry.get('wedding_id')!r}")
            frame = Frame(
                frame_id=raw["frame_id"],
                wedding_id=entry["wedding_id"],
                path=raw["path"],
                timeline_s=float(raw["timeline_s"]),
                scene=scene,
                ritual=raw.get("ritual"),
                camera_id=raw.get("camera_id"),
                duplicate_of=raw.get("duplicate_of"),
            )
            if frame.frame_id in seen_frames:
                raise DatasetError(f"frame {frame.frame_id} appears twice in the manifest")
            seen_frames.add(frame.frame_id)
            frames.append(frame)
        if not frames:
            raise DatasetError(f"wedding {entry.get('wedding_id')!r} has no labelled frames")
        weddings.append(
            Wedding(
                wedding_id=entry["wedding_id"],
                tradition=tradition,
                frames=tuple(sorted(frames, key=lambda f: (f.timeline_s, f.frame_id))),
            )
        )

    if not weddings:
        raise DatasetError("the manifest contains no weddings")
    return tuple(sorted(weddings, key=lambda w: w.wedding_id))


# --------------------------------------------------------------------------
# Splitting
# --------------------------------------------------------------------------


def split_by_wedding(
    weddings: Iterable[Wedding],
    *,
    holdout_tradition: str,
    val_fraction: float = 0.2,
    seed: int = 20260813,
) -> Split:
    """Split by wedding, with one tradition held out entirely for test.

    Every wedding of ``holdout_tradition`` goes to test and nowhere else. The rest
    are shuffled with a fixed seed and divided between train and validation.

    Raises :class:`DatasetError` when the holdout would leave no test weddings, or
    when train would be empty - both of which produce a number that looks like a
    result and is not one.
    """
    weddings = list(weddings)
    if holdout_tradition not in TRADITIONS:
        raise DatasetError(f"unknown tradition {holdout_tradition!r}")

    test = [w for w in weddings if w.tradition == holdout_tradition]
    if not test:
        raise DatasetError(
            f"no weddings of tradition {holdout_tradition!r}; a cross-tradition holdout with "
            "nothing held out is an ordinary split wearing its name"
        )

    remaining = [w for w in weddings if w.tradition != holdout_tradition]
    if not remaining:
        raise DatasetError("every wedding is in the holdout tradition; nothing left to train on")

    # A fixed seed, and shuffling a sorted list, so the split is reproducible on
    # every machine. MLOPS's section 9 task is exactly this.
    remaining.sort(key=lambda w: w.wedding_id)
    generator = random.Random(seed)
    generator.shuffle(remaining)

    val_count = max(1, round(len(remaining) * val_fraction)) if len(remaining) > 1 else 0
    val = remaining[:val_count]
    train = remaining[val_count:]
    if not train:
        raise DatasetError("the validation fraction consumed every training wedding")

    split = Split(train=tuple(train), val=tuple(val), test=tuple(test))
    assert_no_leakage(split)
    return split


def assert_no_leakage(split: Split) -> None:
    """Refuse a split where any wedding appears in more than one fold."""
    folds = {"train": split.train, "val": split.val, "test": split.test}
    seen: dict[str, str] = {}
    for name, weddings in folds.items():
        for wedding in weddings:
            if wedding.wedding_id in seen:
                raise DatasetError(
                    f"wedding {wedding.wedding_id} is in both {seen[wedding.wedding_id]} and "
                    f"{name}; a frame-level split reports memorisation as retrieval"
                )
            seen[wedding.wedding_id] = name


# --------------------------------------------------------------------------
# Pair mining
# --------------------------------------------------------------------------


def positive_pairs(wedding: Wedding, *, window_s: float = POSITIVE_WINDOW_S) -> Iterator[tuple[Frame, Frame]]:
    """Same wedding, same scene (and ritual, when labelled), within ``window_s``.

    Section 6.1's definition, and the reason the timeline is in the label schema:
    "same scene" alone would pair the start of a ceremony with its end, which are
    two different moments that happen to share a word.
    """
    frames = sorted(wedding.frames, key=lambda f: (f.timeline_s, f.frame_id))
    for index, left in enumerate(frames):
        for right in frames[index + 1 :]:
            if right.timeline_s - left.timeline_s > window_s:
                break
            if left.key() == right.key():
                yield (left, right)


def hard_negatives(wedding: Wedding) -> Iterator[tuple[Frame, Frame]]:
    """Different scenes of the *same* wedding.

    Section 6.1 calls these the hard negatives, and the reason they are the hard
    ones is that everything easy about them is shared: the same venue, the same
    light, the same people, the same camera, the same photographer. A negative
    drawn from another wedding is separable on white balance alone, and a head
    trained on those learns to tell weddings apart rather than scenes.
    """
    by_scene: dict[tuple[str, str], list[Frame]] = {}
    for frame in wedding.frames:
        by_scene.setdefault(frame.key(), []).append(frame)
    keys = sorted(by_scene)
    for index, left_key in enumerate(keys):
        for right_key in keys[index + 1 :]:
            for left in by_scene[left_key]:
                for right in by_scene[right_key]:
                    yield (left, right)


def duplicate_ground_truth(weddings: Iterable[Wedding]) -> list[tuple[str, str]]:
    """Every annotated duplicate pair, as ``(frame_id, frame_id)`` sorted within a pair.

    This is what section 6.4's duplicate gate is measured against, and it is
    annotator-provided rather than derived: a pair that a difference hash calls
    identical is not evidence about whether a photographer would call it a
    duplicate.
    """
    pairs: set[tuple[str, str]] = set()
    for wedding in weddings:
        known = {frame.frame_id for frame in wedding.frames}
        for frame in wedding.frames:
            if frame.duplicate_of is None:
                continue
            if frame.duplicate_of not in known:
                raise DatasetError(
                    f"{frame.frame_id} is marked a duplicate of {frame.duplicate_of}, which is "
                    "not in the same wedding"
                )
            pairs.add(tuple(sorted((frame.frame_id, frame.duplicate_of))))
    return sorted(pairs)


# --------------------------------------------------------------------------
# Augmentation policy
# --------------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class Augmentation:
    """One sampled augmentation. Photographic parameters only, by construction.

    There is no ``flip`` field and no ``crop_fraction`` field, and their absence is
    the policy: a value that cannot be represented cannot be set by a hurried
    change to a training script six months from now.
    """

    exposure_ev: float
    white_balance_k: float
    noise_sigma: float
    jpeg_quality: int


#: Section 6.1's bounds.
EXPOSURE_EV_RANGE = (-1.5, 1.5)
WHITE_BALANCE_K_RANGE = (-800.0, 800.0)
NOISE_SIGMA_RANGE = (0.0, 0.01)
JPEG_QUALITY_RANGE = (72, 100)


def sample_augmentation(generator: random.Random) -> Augmentation:
    """Draw one augmentation from the policy.

    Luma-aware sampling, per section 12's mitigation for dark scenes, belongs one
    level up: the *sampler* over frames biases towards dark scenes, and the
    augmentation itself stays uniform. Mixing the two here would make a dark frame
    both more likely to be drawn and more likely to be pushed darker.
    """
    return Augmentation(
        exposure_ev=generator.uniform(*EXPOSURE_EV_RANGE),
        white_balance_k=generator.uniform(*WHITE_BALANCE_K_RANGE),
        noise_sigma=generator.uniform(*NOISE_SIGMA_RANGE),
        jpeg_quality=generator.randint(*JPEG_QUALITY_RANGE),
    )


def luma_aware_weights(frames: Iterable[Frame], dark_scenes: frozenset[str] = frozenset({"dance", "reception"})) -> list[float]:
    """Sampling weights that over-represent the scenes generic embeddings collapse.

    Section 12: "Dark scenes cluster badly -> luma-aware sampling in training plus
    dedicated dark-scene regression fixtures." Without measured luminance the scene
    label is the best available proxy, and it is a good one: a dance floor is dark
    because it is a dance floor.

    A weight of 2.0 rather than 10.0 on purpose. Over-weighting a hard subset far
    enough turns a general embedding into a specialist on it, and the dance floor is
    perhaps a fifth of a wedding.
    """
    return [2.0 if frame.scene in dark_scenes else 1.0 for frame in frames]


# --------------------------------------------------------------------------
# Self-test and CLI
# --------------------------------------------------------------------------


def _synthetic_manifest() -> tuple[Wedding, ...]:
    """A tiny manifest with known structure, for the self-test."""
    weddings = []
    for index, tradition in enumerate(("hindu", "christian", "nepali", "hindu", "christian")):
        frames = []
        base = 1_700_000_000.0 + index * 86_400
        for scene_index, scene in enumerate(("preparation", "ceremony", "ritual", "dance")):
            for member in range(4):
                frame_id = f"w{index}-s{scene_index}-f{member}"
                frames.append(
                    Frame(
                        frame_id=frame_id,
                        wedding_id=f"wedding-{index}",
                        path=f"/nowhere/{frame_id}.arw",
                        timeline_s=base + scene_index * 3_600 + member * 5,
                        scene=scene,
                        ritual="saptapadi" if scene == "ritual" else None,
                        camera_id=f"cam-{member % 2}",
                        duplicate_of=f"w{index}-s{scene_index}-f0" if member == 1 else None,
                    )
                )
        weddings.append(Wedding(wedding_id=f"wedding-{index}", tradition=tradition, frames=tuple(frames)))
    return tuple(weddings)


def _self_test() -> int:
    weddings = _synthetic_manifest()

    split = split_by_wedding(weddings, holdout_tradition="nepali")
    assert len(split.test) == 1, split.counts()
    assert all(w.tradition == "nepali" for w in split.test)
    assert split.train, "no training weddings"
    assert_no_leakage(split)

    # A holdout with nothing in it is refused rather than silently ignored.
    try:
        split_by_wedding(weddings, holdout_tradition="sikh")
    except DatasetError:
        pass
    else:  # pragma: no cover - the refusal is the behaviour under test
        raise AssertionError("an empty holdout was accepted")

    # Leakage is refused.
    try:
        assert_no_leakage(Split(train=weddings[:2], val=weddings[1:3], test=weddings[3:]))
    except DatasetError:
        pass
    else:  # pragma: no cover
        raise AssertionError("a leaking split was accepted")

    # Positives are same scene and inside the window; the window really binds.
    first = weddings[0]
    positives = list(positive_pairs(first))
    assert positives, "no positive pairs"
    for left, right in positives:
        assert left.key() == right.key()
        assert abs(right.timeline_s - left.timeline_s) <= POSITIVE_WINDOW_S
    tight = list(positive_pairs(first, window_s=1.0))
    assert len(tight) < len(positives), "the window has no effect"

    # Hard negatives are the same wedding and different scenes.
    negatives = list(hard_negatives(first))
    assert negatives, "no hard negatives"
    for left, right in negatives:
        assert left.wedding_id == right.wedding_id
        assert left.key() != right.key()

    # Duplicate ground truth is symmetric and de-duplicated.
    duplicates = duplicate_ground_truth(weddings)
    assert duplicates, "no duplicate pairs"
    assert len(duplicates) == len(set(duplicates))
    assert all(pair == tuple(sorted(pair)) for pair in duplicates)

    # The augmentation policy stays inside section 6.1's bounds, and cannot express
    # a flip or a crop at all.
    generator = random.Random(7)
    for _ in range(200):
        augmentation = sample_augmentation(generator)
        assert EXPOSURE_EV_RANGE[0] <= augmentation.exposure_ev <= EXPOSURE_EV_RANGE[1]
        assert WHITE_BALANCE_K_RANGE[0] <= augmentation.white_balance_k <= WHITE_BALANCE_K_RANGE[1]
        assert NOISE_SIGMA_RANGE[0] <= augmentation.noise_sigma <= NOISE_SIGMA_RANGE[1]
        assert JPEG_QUALITY_RANGE[0] <= augmentation.jpeg_quality <= JPEG_QUALITY_RANGE[1]
    assert not hasattr(Augmentation, "flip")
    assert "flip" not in {field.name for field in dataclasses.fields(Augmentation)}
    assert "crop_fraction" not in {field.name for field in dataclasses.fields(Augmentation)}

    # Dark scenes are over-represented, and not by a lot.
    weights = luma_aware_weights(first.frames)
    assert max(weights) == 2.0 and min(weights) == 1.0

    # Two runs of the split agree.
    again = split_by_wedding(weddings, holdout_tradition="nepali")
    assert [w.wedding_id for w in split.train] == [w.wedding_id for w in again.train]

    print("dataset self-test: ok")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--manifest", type=pathlib.Path, help="label manifest to load")
    parser.add_argument("--holdout", default="nepali", choices=TRADITIONS, help="tradition held out for test")
    parser.add_argument("--summary", action="store_true", help="print split and pair counts")
    parser.add_argument("--self-test", action="store_true", help="run the built-in checks")
    args = parser.parse_args(argv)

    if args.self_test:
        return _self_test()

    if args.manifest is None:
        parser.error("--manifest is required unless --self-test is given")

    weddings = load_manifest(args.manifest)
    split = split_by_wedding(weddings, holdout_tradition=args.holdout)
    if args.summary:
        counts = split.counts()
        counts["positive_pairs"] = sum(len(list(positive_pairs(w))) for w in split.train)
        counts["hard_negatives"] = sum(len(list(hard_negatives(w))) for w in split.train)
        counts["duplicate_pairs"] = len(duplicate_ground_truth(weddings))
        print(json.dumps(counts, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
