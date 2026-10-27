"""Score native renders against the finished half of a before/after pair.

Reads the folder `crates/aura-app/tests/reference_pairs.rs` wrote into: for every
`NAME-before.png` / `NAME-after.png` and every `NAME.MODE_WxH.rgb` beside them it reports

- the mean absolute RGB error to the finished image, and the same figure for the
  untouched photograph, so "no edit at all" is always the baseline on the page;
- the same error split into broad tone/colour (a 1/64-edge blur) and fine detail;
- how the edit *moved*: the cosine between AURA's change and the reference's change,
  and what share of the reference's change AURA delivered along that direction.

Nothing is resized or given a tolerance. Whether the two halves of a pair are the
same frame is measured rather than assumed: the finished image is registered onto
the original (an affine fit to local structure, which a grade does not move) and
the largest displacement that fit asks for is printed. Under half a pixel the pair
is scored as it is; `--align` scores against the registered image instead. A
published pair is a lossy marketing export, so its own compression noise is part
of every figure, and none of them is a removal rate or a judgement of quality.
The sheets are for looking at.
"""
import argparse
import json
import re
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter


def load(path):
    return np.asarray(Image.open(path).convert('RGB'), dtype=np.float32)


def blur(pixels, radius):
    image = Image.fromarray(np.clip(pixels, 0, 255).astype(np.uint8))
    return np.asarray(image.filter(ImageFilter.GaussianBlur(radius)), dtype=np.float32)


def box(values, radius):
    """Mean over a square of half-width `radius`, edges replicated."""
    for axis in (0, 1):
        pad = [(0, 0), (0, 0)]
        pad[axis] = (radius + 1, radius)
        total = np.cumsum(np.pad(values, pad, mode='edge'), axis=axis, dtype=np.float64)
        length = values.shape[axis]
        upper = np.take(total, np.arange(2 * radius + 1, 2 * radius + 1 + length), axis=axis)
        lower = np.take(total, np.arange(0, length), axis=axis)
        values = ((upper - lower) / (2 * radius + 1)).astype(np.float32)
    return values


def structure(image, radius=3):
    """Luminance with its local mean and contrast taken out: what survives a grade."""
    values = np.asarray(image.convert('L'), dtype=np.float32)
    smooth = lambda v: box(box(box(v, radius), radius), radius)
    detail = values - smooth(values)
    return detail / (np.sqrt(smooth(detail * detail)) + 2.0)


def warp(values, affine, size):
    image = Image.fromarray(values.astype(np.float32))
    return np.asarray(image.transform(size, Image.AFFINE, tuple(affine.ravel()), resample=Image.BICUBIC),
                      dtype=np.float32)


def register(fixed, moving, levels=5, iterations=40):
    """The affine map from `fixed` pixels to `moving` pixels, coarse to fine (Gauss-Newton)."""
    affine = np.array([[1, 0, 0], [0, 1, 0]], dtype=np.float64)
    for level in reversed(range(levels)):
        scale = 2 ** level
        size = (fixed.width // scale, fixed.height // scale)
        target = structure(fixed.resize(size, Image.LANCZOS))
        source = structure(moving.resize(size, Image.LANCZOS))
        local = affine.copy()
        local[:, 2] /= scale
        ys, xs = np.mgrid[0:size[1], 0:size[0]].astype(np.float32)
        for _ in range(iterations):
            warped = warp(source, local, size)
            gy, gx = np.gradient(warped)
            residual = target - warped
            inside = warp(np.ones_like(source), local, size) > 0.99
            spread = np.median(np.abs(residual[inside])) * 1.4826 + 1e-6
            weight = (inside / (1 + (residual / (3 * spread)) ** 2)).reshape(-1)
            jacobian = np.stack([gx * xs, gx * ys, gx, gy * xs, gy * ys, gy], axis=-1).reshape(-1, 6)
            normal = (jacobian * weight[:, None]).T @ jacobian
            step = np.linalg.solve(normal + 1e-9 * np.eye(6), (jacobian * weight[:, None]).T @ residual.reshape(-1))
            local += step.reshape(2, 3)
            if np.abs(step[[2, 5]]).max() < 1e-3 and np.abs(step[[0, 1, 3, 4]]).max() < 1e-6:
                break
        affine = local.copy()
        affine[:, 2] *= scale
    return affine


def displacement(affine, width, height):
    """The furthest any corner of the frame is moved by `affine`, in pixels."""
    corners = np.array([[0, 0, 1], [width, 0, 1], [0, height, 1], [width, height, 1]], dtype=np.float64)
    return float(np.abs(corners @ affine.T - corners[:, :2]).max())


def moved(result, before, after):
    """Cosine of the two change fields and the delivered share along the reference's."""
    ours, theirs = (result - before).ravel(), (after - before).ravel()
    norm = float(np.linalg.norm(theirs))
    if norm == 0:
        return None, None
    dot = float(ours @ theirs)
    own = float(np.linalg.norm(ours))
    return (dot / (own * norm) if own else 0.0), dot / (norm * norm)


def score(result, before, after, radius):
    low = [blur(x, radius) for x in (result, before, after)]
    high = [x - l for x, l in zip((result, before, after), low)]
    cosine_low, share_low = moved(*low)
    cosine_high, share_high = moved(*high)
    cosine, share = moved(result, before, after)
    return {
        'error': float(np.abs(result - after).mean()),
        'error_untouched': float(np.abs(before - after).mean()),
        'tone_error': float(np.abs(low[0] - low[2]).mean()),
        'tone_error_untouched': float(np.abs(low[1] - low[2]).mean()),
        'detail_error': float(np.abs(high[0] - high[2]).mean()),
        'detail_error_untouched': float(np.abs(high[1] - high[2]).mean()),
        'change_cosine': cosine, 'change_share': share,
        'tone_cosine': cosine_low, 'tone_share': share_low,
        'detail_cosine': cosine_high, 'detail_share': share_high,
    }


def busiest(before, after, size, count):
    """The windows where the reference changed fine detail most: where retouching happened."""
    gap = np.abs((after - blur(after, 6)) - (before - blur(before, 6))).mean(axis=2)
    height, width = gap.shape
    step = size // 2
    windows = []
    for top in range(0, height - size + 1, step):
        for left in range(0, width - size + 1, step):
            windows.append((float(gap[top:top + size, left:left + size].mean()), left, top))
    windows.sort(reverse=True)
    chosen = []
    for _, left, top in windows:
        if len(chosen) >= count:
            break
        if all(abs(left - l) >= size or abs(top - t) >= size for l, t in chosen):
            chosen.append((left, top))
    return chosen


def sheet(panels, path, limit=None):
    """Labelled panels side by side at delivered pixels, shrunk only when asked."""
    images = [Image.fromarray(np.clip(p, 0, 255).astype(np.uint8)) for _, p in panels]
    width, height = images[0].size
    out = Image.new('RGB', (width * len(images), height + 22), 'white')
    draw = ImageDraw.Draw(out)
    for index, ((label, _), image) in enumerate(zip(panels, images)):
        out.paste(image, (index * width, 22))
        draw.text((index * width + 6, 5), label, fill='black')
    if limit:
        out.thumbnail(limit)
    out.save(path, quality=92)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--mode', help='only this workflow')
    parser.add_argument('--crops', type=int, default=3, help='detail crops per pair')
    parser.add_argument('--crop-size', type=int, default=360)
    parser.add_argument('--align', action='store_true', help='score against the registered finished image')
    args = parser.parse_args()
    report = {}
    for before_path in sorted(args.folder.glob('*-before.png')):
        name = before_path.name[:-len('-before.png')]
        before, after = load(before_path), load(args.folder / f'{name}-after.png')
        assert before.shape == after.shape, 'A pair must share its dimensions; nothing is resized'
        original, finished = Image.open(before_path).convert('RGB'), Image.open(args.folder / f'{name}-after.png').convert('RGB')
        affine = register(original, finished)
        offset = displacement(affine, original.width, original.height)
        print(f'{name:10} registration: the finished image sits within {offset:.2f} px of the original'
              + ('' if offset < 0.5 else ' - NOT the same frame; use --align'))
        if args.align:
            after = np.asarray(finished.transform(original.size, Image.AFFINE, tuple(affine.ravel()),
                                                  resample=Image.BICUBIC), dtype=np.float32)
        report[f'{name}.registration'] = {'affine': affine.tolist(), 'largest_displacement_px': offset}
        radius = max(before.shape[0], before.shape[1]) / 64
        for raw in sorted(args.folder.glob(f'{name}.*_*x*.rgb')):
            mode, width, height = re.fullmatch(rf'{re.escape(name)}\.(\w+)_(\d+)x(\d+)\.rgb', raw.name).groups()
            if args.mode and mode != args.mode:
                continue
            result = np.frombuffer(raw.read_bytes(), dtype=np.uint8).reshape(int(height), int(width), 3).astype(np.float32)
            assert result.shape == after.shape, 'The render must keep the photograph dimensions'
            row = score(result, before, after, radius)
            report[f'{name}.{mode}'] = row
            Image.fromarray(result.astype(np.uint8)).save(args.folder / f'{name}.{mode}.png')
            panels = [('Original', before), (f'AURA {mode}', result), ('Reference', after)]
            sheet(panels, args.folder / f'{name}.{mode}.sheet.jpg', (2400, 2400))
            for index, (left, top) in enumerate(busiest(before, after, args.crop_size, args.crops)):
                box = (slice(top, top + args.crop_size), slice(left, left + args.crop_size))
                sheet([(label, pixels[box]) for label, pixels in panels],
                      args.folder / f'{name}.{mode}.crop{index}.jpg')
            share = lambda v: 'n/a' if v is None else f'{v:+.2f}'
            print(f"{name:10} {mode:8} error {row['error']:.2f} (untouched {row['error_untouched']:.2f})  "
                  f"tone {row['tone_error']:.2f} ({row['tone_error_untouched']:.2f})  "
                  f"detail {row['detail_error']:.2f} ({row['detail_error_untouched']:.2f})  "
                  f"tone cos {share(row['tone_cosine'])} share {share(row['tone_share'])}  "
                  f"detail cos {share(row['detail_cosine'])} share {share(row['detail_share'])}")
    (args.folder / 'comparison.json').write_text(json.dumps(report, indent=2), encoding='utf8')


if __name__ == '__main__':
    main()
