"""Assemble per-frame scene training examples from a catalog.

The scene heads are **adapters on a frozen trunk** - PHASE-07 section 6.1 - so an
example is not a photograph. It is a 512-d embedding that phase 05 already computed,
sixteen context numbers the catalog already holds, and a label. That is the whole
reason this phase costs 35 seconds for four thousand images where phase 06's face pass
costs twelve minutes.

    python ml/models/scene/dataset.py --catalog wedding.sqlite --out scenes.npz
    python ml/models/scene/dataset.py --self-test

## The sixteen context slots, which are a contract

The order below is repeated in `crates/aura-brain-wedding/src/scene/attributes.rs` and
in `docs/model-cards/scene_classifier.md`. **Reordering it silently relabels every
stored posterior**, so it is written down three times on purpose and a test asserts the
Rust side against this file's `CONTEXT_SLOTS`.

| Slots | Feature | Neutral when unknown |
|---|---|---|
| 0, 1 | hour of the wedding, sine and cosine over 24 h | (0, 0) |
| 2 | fraction of a day since the first frame, saturating | 0 |
| 3 | flash fired, from EXIF | 0.5 |
| 4-7 | ISO bucket, one-hot: base / 800 / 3200 / above | all zero |
| 8 | face count from phase 06, saturating at twelve | 0 |
| 9 | one of the couple is present, from `PeopleService` | 0.5 |
| 10-13 | luminance shape: p50, p1, p99, total clipping | all zero |
| 14, 15 | colour temperature, normalised, plus a present flag | (0, 0) |

Two slots for the hour rather than one, because a wedding that runs past midnight is
the common case for three of the five shipped traditions, and a linear feature would put
the vidaai at the opposite end of the day from the saptapadi half an hour before it.

## The split rule, which matters more than the architecture

**Wedding-level splits, never frame-level.** Two frames from the same ceremony three
seconds apart are near-duplicates; a frame-level split puts one in train and one in test
and reports an accuracy that is a measurement of leakage. `split_by_wedding` is the only
splitter here and it refuses a request for a frame-level one.

This is the same rule phase 05's `ml/models/embed/dataset.py` applies to retrieval and
phase 06's applies to identity, for the same reason each time.

## What this script cannot do today

There is no labelled wedding data in this repository. This is condition **C2** of
`docs/progress/PHASE-07-EXIT.md`. What is here is the assembly, the split rule, the
class-balance report and the self-test - everything except the labels, so that the day a
labelled corpus exists the argument is about the data rather than about the code.
"""

from __future__ import annotations

import argparse
import json
import math
import sqlite3
import sys
from dataclasses import dataclass, field
from typing import Iterable, Sequence

# The 22 scene classes, in `SceneId::ALL` order. This IS the model's output order.
SCENES: tuple[str, ...] = (
    "getting_ready_bride",
    "getting_ready_groom",
    "details",
    "first_look",
    "ceremony_entrance",
    "ceremony",
    "ritual",
    "vows",
    "rings",
    "kiss",
    "family_portrait",
    "group_portrait",
    "couple_portrait",
    "golden_hour",
    "reception_entrance",
    "speeches",
    "cake",
    "first_dance",
    "dance_floor",
    "candid",
    "venue",
    "exit",
)

# The 14 attributes, in `AttrFlags::ALL` order.
ATTRIBUTES: tuple[str, ...] = (
    "indoor",
    "flash",
    "mixed_light",
    "backlit",
    "night",
    "tungsten",
    "crowd",
    "stage",
    "vehicle",
    "food",
    "decor",
    "kids",
    "pets",
    "rain",
)

CONTEXT_SLOTS = 16
EMBED_DIM = 512
FEATURE_DIM = EMBED_DIM + CONTEXT_SLOTS

# The ISO bucket boundaries, which are where photographic behaviour changes rather than
# where the arithmetic is smooth: below 800 is daylight, 800-3200 is an indoor ceremony,
# above 3200 is a dark reception.
ISO_BUCKETS = (800, 3200, 12800)


@dataclass
class Frame:
    """One training example."""

    photo_id: str
    wedding_id: str
    embedding: list[float]
    context: list[float]
    scene: str | None = None
    attributes: list[str] = field(default_factory=list)
    ritual: str | None = None
    tradition: str | None = None

    def features(self) -> list[float]:
        """The 528-wide vector the scene head takes."""
        vector = list(self.embedding[:EMBED_DIM])
        vector.extend([0.0] * (EMBED_DIM - len(vector)))
        vector.extend(self.context[:CONTEXT_SLOTS])
        vector.extend([0.0] * (FEATURE_DIM - len(vector)))
        return vector


def context_vector(
    *,
    offset_ms: int | None = None,
    flash: bool | None = None,
    iso: int | None = None,
    kelvin: int | None = None,
    faces: int | None = None,
    has_primary: bool | None = None,
    luma: Sequence[float] | None = None,
) -> list[float]:
    """The sixteen context numbers, in the documented order.

    Every argument is optional and every omission produces a *documented neutral* rather
    than a zero, because zero is a measurement for several of these and "unknown" is not.
    """
    out = [0.0] * CONTEXT_SLOTS

    if offset_ms is not None:
        hours = offset_ms / 3_600_000.0
        angle = hours * math.tau / 24.0
        out[0] = math.sin(angle)
        out[1] = math.cos(angle)
        out[2] = min(max(offset_ms / 86_400_000.0, 0.0), 1.0)

    out[3] = 0.5 if flash is None else (1.0 if flash else 0.0)

    if iso is not None:
        if iso < ISO_BUCKETS[0]:
            out[4] = 1.0
        elif iso < ISO_BUCKETS[1]:
            out[5] = 1.0
        elif iso < ISO_BUCKETS[2]:
            out[6] = 1.0
        else:
            out[7] = 1.0

    if faces is not None:
        out[8] = min(max(faces / 12.0, 0.0), 1.0)

    out[9] = 0.5 if has_primary is None else (1.0 if has_primary else 0.0)

    if luma is not None and len(luma) >= 4:
        # p50, p1, p99, clip_lo + clip_hi
        out[10] = min(max(luma[0], 0.0), 1.0)
        out[11] = min(max(luma[1], 0.0), 1.0)
        out[12] = min(max(luma[2], 0.0), 1.0)
        out[13] = min(max(luma[3], 0.0), 1.0)

    if kelvin is not None:
        out[14] = min(max((kelvin - 2000.0) / 8000.0, 0.0), 1.0)
        out[15] = 1.0

    return out


def split_by_wedding(
    frames: Sequence[Frame], *, holdout: float = 0.2, seed: int = 7
) -> tuple[list[Frame], list[Frame]]:
    """Split train and test by wedding, never by frame.

    Deterministic: the weddings are sorted and hashed with a fixed seed, so two runs
    produce the same split and a reported accuracy is reproducible.
    """
    weddings = sorted({frame.wedding_id for frame in frames})
    if not weddings:
        return [], []
    # A stable hash rather than Python's, which is salted per process.
    ranked = sorted(
        weddings,
        key=lambda name: (
            int.from_bytes(
                __import__("hashlib").blake2b(
                    f"{seed}:{name}".encode(), digest_size=8
                ).digest(),
                "big",
            ),
            name,
        ),
    )
    cut = max(1, round(len(ranked) * holdout))
    held = set(ranked[:cut])
    train = [frame for frame in frames if frame.wedding_id not in held]
    test = [frame for frame in frames if frame.wedding_id in held]
    return train, test


def split_by_frame(_frames: Sequence[Frame]) -> tuple[list[Frame], list[Frame]]:
    """Refused. See this module's docstring."""
    raise SystemExit(
        "frame-level splits are refused: two frames from the same ceremony three seconds "
        "apart are near-duplicates, and a frame-level split reports a measurement of "
        "leakage. Use split_by_wedding."
    )


def class_balance(frames: Sequence[Frame]) -> dict[str, int]:
    """How many examples each scene has.

    Reported before training, always. Section 6.1 asks for focal loss and oversampling
    because `kiss` and `first_look` have two orders of magnitude fewer examples than
    `candid`, and a training run that has not looked at this table is a training run
    about the majority class.
    """
    counts = {scene: 0 for scene in SCENES}
    for frame in frames:
        if frame.scene in counts:
            counts[frame.scene] += 1
    return counts


def thin_classes(frames: Sequence[Frame], *, floor: int = 200) -> list[str]:
    """Scenes with too few examples for section 10.1's per-class gate.

    Section 10.1 gates per-class accuracy at 0.85 for classes with more than 200
    labelled samples. Below that the gate does not apply, and a training report that did
    not say which classes fell below it would look like a clean pass.
    """
    counts = class_balance(frames)
    return [scene for scene, count in counts.items() if count < floor]


def read_catalog(path: str) -> list[Frame]:
    """Assemble unlabelled examples from a catalog.

    Unlabelled on purpose: the embedding and the context come from the catalog, and the
    label comes from a human. This produces the left-hand side of the training set and a
    labelling tool produces the right.
    """
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    try:
        rows = conn.execute(
            """
            SELECT p.photo_id, p.project_id, p.timeline_time, p.iso, p.flash_fired,
                   e.vec,
                   d.luma_p50, d.luma_p1, d.luma_p99, d.clip_lo, d.clip_hi,
                   fs.faces_found
              FROM photo p
              JOIN embeddings e   ON e.photo_id = p.photo_id
              LEFT JOIN descriptors d ON d.photo_id = p.photo_id
              LEFT JOIN face_scan fs  ON fs.photo_id = p.photo_id
             ORDER BY p.project_id, p.timeline_time, p.photo_id
            """
        ).fetchall()
    finally:
        conn.close()

    day_start: dict[str, float] = {}
    frames: list[Frame] = []
    for row in rows:
        wedding = row["project_id"]
        stamp = _parse_ms(row["timeline_time"])
        if stamp is not None:
            day_start.setdefault(wedding, stamp)
        offset = None if stamp is None else int(stamp - day_start.get(wedding, stamp))

        luma = None
        if row["luma_p50"] is not None:
            luma = (
                row["luma_p50"],
                row["luma_p1"],
                row["luma_p99"],
                (row["clip_lo"] or 0.0) + (row["clip_hi"] or 0.0),
            )

        frames.append(
            Frame(
                photo_id=row["photo_id"],
                wedding_id=wedding,
                embedding=_decode_f16(row["vec"]),
                context=context_vector(
                    offset_ms=offset,
                    flash=None if row["flash_fired"] is None else bool(row["flash_fired"]),
                    iso=row["iso"],
                    faces=row["faces_found"],
                    luma=luma,
                ),
            )
        )
    return frames


def _parse_ms(text: str | None) -> float | None:
    """RFC 3339 to epoch milliseconds, without a dependency."""
    if not text:
        return None
    try:
        from datetime import datetime, timezone

        cleaned = text.replace("Z", "+00:00")
        return datetime.fromisoformat(cleaned).replace(tzinfo=timezone.utc).timestamp() * 1000.0
    except ValueError:
        return None


def _decode_f16(blob: bytes | None) -> list[float]:
    """The stored half-precision vector, as floats.

    Hand-decoded rather than via numpy, so this file runs anywhere Python does. The bit
    layout is `aura_index::contract::index::F16`'s, which is IEEE 754 binary16.
    """
    if not blob:
        return [0.0] * EMBED_DIM
    out: list[float] = []
    for index in range(0, min(len(blob), EMBED_DIM * 2), 2):
        bits = blob[index] | (blob[index + 1] << 8)
        sign = -1.0 if bits & 0x8000 else 1.0
        exponent = (bits >> 10) & 0x1F
        mantissa = bits & 0x03FF
        if exponent == 0:
            value = sign * (mantissa / 1024.0) * (2.0**-14)
        elif exponent == 0x1F:
            value = sign * float("inf") if mantissa == 0 else float("nan")
        else:
            value = sign * (1.0 + mantissa / 1024.0) * (2.0 ** (exponent - 15))
        out.append(value)
    out.extend([0.0] * (EMBED_DIM - len(out)))
    return out


def write_npz(frames: Iterable[Frame], path: str) -> int:
    """Write a training set, or a JSON fallback when numpy is absent."""
    materialised = list(frames)
    payload = {
        "features": [frame.features() for frame in materialised],
        "scene": [frame.scene for frame in materialised],
        "attributes": [frame.attributes for frame in materialised],
        "ritual": [frame.ritual for frame in materialised],
        "wedding": [frame.wedding_id for frame in materialised],
    }
    try:
        import numpy as np

        np.savez_compressed(
            path,
            features=np.asarray(payload["features"], dtype="float32"),
            scene=np.asarray([s or "" for s in payload["scene"]]),
            wedding=np.asarray(payload["wedding"]),
        )
    except ImportError:
        with open(path + ".json", "w", encoding="utf-8") as handle:
            json.dump(payload, handle)
    return len(materialised)


def _self_test() -> int:
    """Prove the two things this file is responsible for being right about."""
    failures = 0

    # 1. The context vector's shape and its neutrals.
    empty = context_vector()
    if len(empty) != CONTEXT_SLOTS:
        print(f"FAIL: context vector is {len(empty)} slots, not {CONTEXT_SLOTS}")
        failures += 1
    if empty[3] != 0.5 or empty[9] != 0.5:
        print("FAIL: an unknown flash or couple presence is not the honest midpoint")
        failures += 1
    if any(empty[4:8]):
        print("FAIL: an unknown ISO set a bucket")
        failures += 1

    # The hour is cyclic: 23:50 and 00:10 must be close, not a day apart.
    late = context_vector(offset_ms=int(23.83 * 3_600_000))
    early = context_vector(offset_ms=int(0.17 * 3_600_000))
    gap = math.dist(late[:2], early[:2])
    if gap > 0.2:
        print(f"FAIL: 23:50 and 00:10 are {gap:.3f} apart on the hour circle")
        failures += 1

    # 2. The split rule.
    frames = [
        Frame(f"pht_{i}", f"prj_{i // 10}", [0.0] * EMBED_DIM, context_vector())
        for i in range(100)
    ]
    train, test = split_by_wedding(frames)
    train_weddings = {frame.wedding_id for frame in train}
    test_weddings = {frame.wedding_id for frame in test}
    if train_weddings & test_weddings:
        print("FAIL: a wedding appears in both train and test")
        failures += 1
    if not test:
        print("FAIL: the holdout is empty")
        failures += 1
    again, _ = split_by_wedding(frames)
    if [f.photo_id for f in again] != [f.photo_id for f in train]:
        print("FAIL: the split is not deterministic")
        failures += 1

    # 3. The vocabularies match the Rust side's arity.
    if len(SCENES) != 22 or len(ATTRIBUTES) != 14:
        print("FAIL: the vocabularies do not match SceneId::CLASSES and AttrFlags::COUNT")
        failures += 1
    if len(set(SCENES)) != len(SCENES):
        print("FAIL: a scene slug appears twice")
        failures += 1

    print("dataset self-test:", "ok" if failures == 0 else f"{failures} failures")
    return 1 if failures else 0


def main(argv: Sequence[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", help="an AURA catalog to read embeddings from")
    parser.add_argument("--out", default="scenes.npz")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return _self_test()
    if not args.catalog:
        parser.error("--catalog or --self-test")

    frames = read_catalog(args.catalog)
    written = write_npz(frames, args.out)
    counts = class_balance(frames)
    print(f"{written} examples written to {args.out}")
    print("class balance (labels are absent until a corpus exists):")
    for scene, count in counts.items():
        print(f"  {scene:22s} {count}")
    thin = thin_classes(frames)
    if thin:
        print(
            "below section 10.1's 200-sample floor, so the per-class gate does not apply "
            "to them: " + ", ".join(thin)
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
