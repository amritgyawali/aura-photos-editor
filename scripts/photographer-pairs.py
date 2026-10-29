"""Score AURA's renders against the photographer's own finished photographs. ADR-0191.

The photographer's JPEG is aligned onto AURA's render (they may crop or straighten), then both
are compared in CIELAB on the overlap: lightness at five percentiles, the colour cast (mean a*
and b*), colourfulness (median chroma) and the mean colour difference per pixel.

    python scripts/photographer-pairs.py score  <sample.json> <dir> <variant> [<variant> ...]
    python scripts/photographer-pairs.py table  <dir> <variant> [<variant> ...]

<dir> holds `<NAME>-<variant>.png` from `crates/aura-app/tests/photographer_pairs.rs`; `shot`
is the camera file as shot. Alignment is computed once per photograph (on `shot`) and kept in
`<dir>/align.json`; scores go to `<dir>/scores-<variant>.json`.
"""
import json
import os
import sys

import cv2
import numpy as np

EDGE = 960


def load(path, edge=EDGE):
    im = cv2.imread(path, cv2.IMREAD_COLOR)
    if im is None:
        return None
    h, w = im.shape[:2]
    s = edge / max(h, w)
    if abs(s - 1) > 1e-3:
        im = cv2.resize(im, (round(w * s), round(h * s)), interpolation=cv2.INTER_AREA)
    return im


def structure(im):
    g = cv2.cvtColor(im, cv2.COLOR_BGR2GRAY)
    g = cv2.createCLAHE(clipLimit=3.0, tileGridSize=(8, 8)).apply(g)
    return g


def align(shot, theirs):
    """A similarity transform taking the photographer's pixels onto the shot, or None."""
    a, b = structure(shot), structure(theirs)
    orb = cv2.ORB_create(4000)
    ka, da = orb.detectAndCompute(a, None)
    kb, db = orb.detectAndCompute(b, None)
    if da is None or db is None or len(ka) < 30 or len(kb) < 30:
        return None
    m = cv2.BFMatcher(cv2.NORM_HAMMING, crossCheck=True).match(db, da)
    if len(m) < 30:
        return None
    src = np.float32([kb[x.queryIdx].pt for x in m])
    dst = np.float32([ka[x.trainIdx].pt for x in m])
    t, inl = cv2.estimateAffinePartial2D(src, dst, method=cv2.RANSAC, ransacReprojThreshold=4.0)
    if t is None or inl is None or inl.sum() < 25:
        return None
    return t.tolist(), int(inl.sum())


YUNET = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'assets', 'models', 'yunet', 'face_detection_yunet_2023mar.onnx')


def faces_of(im):
    """Face boxes [x, y, w, h] in pixels, found by the same YuNet model AURA ships."""
    h, w = im.shape[:2]
    det = cv2.FaceDetectorYN.create(YUNET, '', (w, h), 0.7, 0.3, 5000)
    _, found = det.detect(im)
    if found is None:
        return []
    return [[float(v) for v in f[:4]] for f in found if f[2] * f[3] > (0.0004 * w * h)]


def face_mask(shape, faces):
    """The middle of each face (the skin, not the hair or the background around it)."""
    m = np.zeros(shape[:2], bool)
    for x, y, w, h in faces:
        x0, x1 = int(x + 0.2 * w), int(x + 0.8 * w)
        y0, y1 = int(y + 0.25 * h), int(y + 0.85 * h)
        m[max(y0, 0):max(y1, 0), max(x0, 0):max(x1, 0)] = True
    return m


def lab(im):
    f = im.astype(np.float32) / 255.0
    return cv2.cvtColor(f, cv2.COLOR_BGR2Lab)


def stats(l, mask):
    L, A, B = l[..., 0][mask], l[..., 1][mask], l[..., 2][mask]
    chroma = np.hypot(A, B)
    return {
        'L': [float(np.percentile(L, p)) for p in (2, 10, 50, 90, 98)],
        'a': float(A.mean()),
        'b': float(B.mean()),
        'chroma': float(np.median(chroma)),
        'clip': float((L > 99.0).mean()),
        'crush': float((L < 2.0).mean()),
    }


def compare(mine, theirs, t, faces=()):
    h, w = mine.shape[:2]
    warped = cv2.warpAffine(theirs, np.float32(t), (w, h), flags=cv2.INTER_AREA, borderValue=(0, 0, 0))
    valid = cv2.warpAffine(np.full(theirs.shape[:2], 255, np.uint8), np.float32(t), (w, h), flags=cv2.INTER_NEAREST)
    mask = cv2.erode(valid, np.ones((9, 9), np.uint8)) > 0
    if mask.mean() < 0.3:
        return None
    lm, lt = lab(mine), lab(warped)
    d = lm - lt
    de = np.sqrt((d ** 2).sum(axis=2))[mask]
    # Lightness difference by region (4x4): how differently the light is shaped.
    regional = []
    for gy in range(4):
        for gx in range(4):
            ys, xs = slice(gy * h // 4, (gy + 1) * h // 4), slice(gx * w // 4, (gx + 1) * w // 4)
            mk = mask[ys, xs]
            if mk.mean() > 0.5:
                regional.append(float(np.median(d[ys, xs, 0][mk])))
    sm, st = stats(lm, mask), stats(lt, mask)
    fm = face_mask(mine.shape, faces) & mask
    face = None
    if fm.sum() > 200:
        face = {
            'mine': [float(np.median(lm[..., k][fm])) for k in range(3)],
            'theirs': [float(np.median(lt[..., k][fm])) for k in range(3)],
        }
    return {
        'face': face,
        'mine': sm,
        'theirs': st,
        'dE': float(de.mean()),
        'dE_median': float(np.median(de)),
        'dL': [m - t_ for m, t_ in zip(sm['L'], st['L'])],
        'da': sm['a'] - st['a'],
        'db': sm['b'] - st['b'],
        'dchroma': sm['chroma'] - st['chroma'],
        'dL_regional_spread': float(np.std(regional)) if regional else None,
    }


def score(sample_path, folder, variants):
    sample = json.load(open(sample_path))
    by_name = {s['name']: s for s in sample}
    align_path = os.path.join(folder, 'align.json')
    aligned = json.load(open(align_path)) if os.path.exists(align_path) else {}
    faces_path = os.path.join(folder, 'faces.json')
    found = json.load(open(faces_path)) if os.path.exists(faces_path) else {}
    for variant in variants:
        out = {}
        for name, s in by_name.items():
            mine_path = os.path.join(folder, f'{name}-{variant}.png')
            shot_path = os.path.join(folder, f'{name}-shot.png')
            if not os.path.exists(mine_path) or not os.path.exists(shot_path):
                continue
            theirs = load(s['jpg'])
            shot = load(shot_path)
            if theirs is None or shot is None:
                continue
            if name not in aligned:
                aligned[name] = align(shot, theirs)
            if not aligned[name]:
                continue
            mine = load(mine_path)
            if mine.shape != shot.shape:
                mine = cv2.resize(mine, (shot.shape[1], shot.shape[0]), interpolation=cv2.INTER_AREA)
            if name not in found:
                found[name] = faces_of(shot)
            r = compare(mine, theirs, aligned[name][0], found[name])
            if r:
                r['cond'] = s['cond']
                r['model'] = s['model']
                out[name] = r
        json.dump(out, open(os.path.join(folder, f'scores-{variant}.json'), 'w'), indent=0)
        print(variant, len(out), 'scored')
    json.dump(aligned, open(align_path, 'w'))
    json.dump(found, open(faces_path, 'w'))


def table(folder, variants):
    rows = {v: json.load(open(os.path.join(folder, f'scores-{v}.json'))) for v in variants}
    names = sorted(set.intersection(*(set(r) for r in rows.values())))
    print(f'{len(names)} photographs scored in every variant')
    hdr = f"{'variant':<10} {'dE':>6} {'dE med':>7} {'|dL50|':>7} {'dL50':>6} {'dL10':>6} {'dL90':>6} {'dL98':>6} {'|cast|':>7} {'da':>6} {'db':>6} {'dchr':>6} {'spread':>7} {'clip':>6}"
    print(hdr)
    for v, r in rows.items():
        g = [r[n] for n in names]
        m = lambda f: float(np.mean([f(x) for x in g]))
        print(f"{v:<10} {m(lambda x: x['dE']):6.2f} {m(lambda x: x['dE_median']):7.2f} {m(lambda x: abs(x['dL'][2])):7.2f} "
              f"{m(lambda x: x['dL'][2]):6.2f} {m(lambda x: x['dL'][1]):6.2f} {m(lambda x: x['dL'][3]):6.2f} {m(lambda x: x['dL'][4]):6.2f} "
              f"{m(lambda x: np.hypot(x['da'], x['db'])):7.2f} {m(lambda x: x['da']):6.2f} {m(lambda x: x['db']):6.2f} {m(lambda x: x['dchroma']):6.2f} "
              f"{m(lambda x: x['dL_regional_spread'] or 0):7.2f} {m(lambda x: x['mine']['clip'] - x['theirs']['clip']):6.3f}")
    fn = [n for n in names if all(rows[v][n]['face'] for v in variants)]
    print()
    print(f'faces ({len(fn)} photographs with a face): skin lightness, mine - theirs; skin colour difference')
    for v, r in rows.items():
        dl = [r[n]['face']['mine'][0] - r[n]['face']['theirs'][0] for n in fn]
        de = [float(np.hypot(r[n]['face']['mine'][1] - r[n]['face']['theirs'][1], r[n]['face']['mine'][2] - r[n]['face']['theirs'][2])) for n in fn]
        print(f"{v:<10} dL {np.mean(dl):6.2f}  |dL| {np.mean(np.abs(dl)):6.2f}  |dab| {np.mean(de):6.2f}")
    # By condition.
    conds = sorted(set(tuple(rows[variants[0]][n]['cond']) for n in names))
    print('\nmean colour difference (dE) by condition')
    print(f"{'condition':<24} {'n':>3} " + ' '.join(f'{v:>9}' for v in variants))
    for c in conds:
        ns = [n for n in names if tuple(rows[variants[0]][n]['cond']) == c]
        print(f"{' '.join(c):<24} {len(ns):>3} " + ' '.join(f"{np.mean([rows[v][n]['dE'] for n in ns]):9.2f}" for v in variants))
    models = sorted(set(str(rows[variants[0]][n]['model']) for n in names))
    print('\nby camera')
    for c in models:
        ns = [n for n in names if str(rows[variants[0]][n]['model']) == c]
        print(f"{c:<24} {len(ns):>3} " + ' '.join(f"{np.mean([rows[v][n]['dE'] for n in ns]):9.2f}" for v in variants))


if __name__ == '__main__':
    if sys.argv[1] == 'score':
        score(sys.argv[2], sys.argv[3], sys.argv[4:])
    else:
        table(sys.argv[2], sys.argv[3:])
