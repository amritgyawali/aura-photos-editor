"""Derive controlled shooting conditions from before/after pairs, with the finished version kept.

For every `NAME-before.png` with a `NAME-after.png` in a folder, writes the pair again under
each condition as `NAME_CONDITION-before.png` and `NAME_CONDITION-after.png`:

    asis    the photograph as it is
    under   one stop under-exposed
    over    0.6 of a stop over-exposed (highlights clip, as they do in a camera)
    warm    a tungsten-like cast: red up and blue down by 12%
    cool    the reverse
    green   a fluorescent-like cast: green up by 8%
    flat    low contrast with lifted blacks, as haze or a flat picture profile gives

Exposure and casts are applied in linear light, which is where a camera makes them. The
finished half is copied unchanged: it is what a correct automatic edit of the damaged half
should come back towards, so `compare-reference-pairs.py` scores a result under a condition
exactly as it scores the original pair.

    python scripts/make-condition-variants.py <pairs> <out> [--long-edge 1000]

A smaller long edge makes a light-and-colour run over the whole matrix quick; use the
original size when the retouch is what is being judged.
"""
import argparse
from pathlib import Path

import numpy as np
from PIL import Image


def decode(encoded):
    v = encoded / 255.0
    return np.where(v <= 0.04045, v / 12.92, ((v + 0.055) / 1.055) ** 2.4)


def encode(linear):
    v = np.clip(linear, 0.0, 1.0)
    return np.round(np.where(v <= 0.0031308, v * 12.92, 1.055 * v ** (1 / 2.4) - 0.055) * 255.0).astype(np.uint8)


def flat(linear):
    display = np.clip(linear, 0.0, 1.0) ** (1 / 2.2)
    return (0.08 + 0.5 + (display - 0.5) * 0.7) ** 2.2 - 0.0


CONDITIONS = {
    'asis': lambda v: v,
    'under': lambda v: v * 0.5,
    'over': lambda v: v * 2 ** 0.6,
    'warm': lambda v: v * np.array([1.12, 1.0, 0.88]),
    'cool': lambda v: v * np.array([0.88, 1.0, 1.12]),
    'green': lambda v: v * np.array([1.0, 1.08, 1.0]),
    'flat': flat,
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pairs', type=Path)
    parser.add_argument('out', type=Path)
    parser.add_argument('--long-edge', type=int, default=0)
    parser.add_argument('--only', help='comma-separated conditions')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    wanted = args.only.split(',') if args.only else list(CONDITIONS)
    for before in sorted(args.pairs.glob('*-before.png')):
        name = before.name[:-len('-before.png')]
        after = before.with_name(f'{name}-after.png')
        if not after.is_file():
            continue
        first, finished = Image.open(before).convert('RGB'), Image.open(after).convert('RGB')
        if args.long_edge and max(first.size) > args.long_edge:
            scale = args.long_edge / max(first.size)
            size = (round(first.width * scale), round(first.height * scale))
            first, finished = first.resize(size, Image.LANCZOS), finished.resize(size, Image.LANCZOS)
        linear = decode(np.asarray(first, dtype=np.float64))
        for condition in wanted:
            Image.fromarray(encode(CONDITIONS[condition](linear))).save(args.out / f'{name}_{condition}-before.png')
            finished.save(args.out / f'{name}_{condition}-after.png')
        print(name, first.size, wanted)


if __name__ == '__main__':
    main()
