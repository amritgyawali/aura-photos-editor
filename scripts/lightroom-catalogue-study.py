"""Read Lightroom catalogues (read-only copies) and summarise how every photo was developed.

    python scripts/lightroom-catalogue-study.py OUT.json CAT1.lrcat [CAT2.lrcat ...]

Open copies, never a catalogue Lightroom is using: the database is opened read-only.

Writes one row per photo (settings + EXIF + shoot) to OUT.json and prints a summary.
"""
import json
import re
import sqlite3
import statistics
import sys
from collections import Counter, defaultdict

SCALAR = re.compile(r'^(\w+) = (".*?"|-?\d+(?:\.\d+)?|true|false)\s*,?\s*\}?\s*$')


def parse(text):
    """Top-level scalars, the top-level tone curve and counts of nested lists."""
    out, depth, key, buf = {}, 0, None, []
    for line in (text or '').splitlines():
        line = line.strip()
        if depth == 1:
            m = SCALAR.match(line)
            if m:
                k, v = m.groups()
                out[k] = v.strip('"') if v.startswith('"') else (v == 'true' if v in ('true', 'false') else float(v))
            elif ' = {' in line:
                key, buf = line.split(' = ', 1)[0], []
        if depth >= 2 and key:
            buf.append(line)
        opened, closed = line.count('{'), line.count('}')
        if line.startswith('s = {'):
            depth = 1
            continue
        depth += opened - closed
        if key and depth == 1 and (opened or closed):
            body = ' '.join(buf)
            if key == 'ToneCurvePV2012':
                out['curve'] = [float(n) for n in re.findall(r'-?\d+(?:\.\d+)?', body)]
            elif key in ('MaskGroupBasedCorrections', 'RetouchInfo', 'RedEyeInfo', 'PaintBasedCorrections',
                         'GradientBasedCorrections', 'CircularGradientBasedCorrections'):
                out[key + 'Count'] = body.count('What =') or body.count('CorrectionActive') or (1 if body.strip(' {},') else 0)
            elif key == 'Look':
                n = re.search(r'Name = "([^"]+)"', body)
                if n:
                    out['LookName'] = n.group(1)
            key = None if depth <= 1 else key
    return out


def rows(path):
    c = sqlite3.connect(f'file:{path}?mode=ro', uri=True)
    q = """select i.id_local, i.captureTime, i.fileFormat, i.rating, i.pick, f.baseName, f.extension,
                  fo.pathFromRoot, r.absolutePath, d.text, d.whiteBalance, d.hasDevelopAdjustmentsEx,
                  e.isoSpeedRating, e.aperture, e.shutterSpeed, e.flashFired, e.focalLength, cm.value
           from Adobe_images i
           join AgLibraryFile f on f.id_local = i.rootFile
           join AgLibraryFolder fo on fo.id_local = f.folder
           join AgLibraryRootFolder r on r.id_local = fo.rootFolder
           left join Adobe_imageDevelopSettings d on d.image = i.id_local
           left join AgHarvestedExifMetadata e on e.image = i.id_local
           left join AgInternedExifCameraModel cm on cm.id_local = e.cameraModelRef"""
    for r in c.execute(q):
        (iid, cap, fmt, rating, pick, base, ext, sub, root, text, wb, adj, iso, ap, sh, flash, fl, cam) = r
        s = parse(text)
        yield {
            'catalog': path, 'id': iid, 'capture': cap, 'format': fmt, 'rating': rating, 'pick': pick,
            'file': f'{root}{sub}{base}.{ext}', 'shoot': root.rstrip('/').split('/')[-1] if root else '',
            'root': root, 'iso': iso, 'aperture': ap, 'shutter': sh, 'flash': flash, 'focal': fl, 'camera': cam,
            'edited': bool(adj), 'wb_mode': s.get('WhiteBalance', wb), 's': s,
        }


def q(values, p):
    v = sorted(values)
    return v[min(len(v) - 1, int(len(v) * p))] if v else None


def summary(data):
    edited = [d for d in data if d['edited']]
    print(f'photos {len(data)}, developed {len(edited)}')
    print('formats', Counter(d['format'] for d in data).most_common())
    print('cameras', Counter(d['camera'] for d in data).most_common(6))
    print('white balance modes', Counter(d['wb_mode'] for d in edited).most_common())
    print('profiles', Counter(d['s'].get('CameraProfile') for d in edited).most_common(6))
    print('looks', Counter(d['s'].get('LookName') for d in edited).most_common(6))
    keys = ['Temperature', 'Tint', 'Exposure2012', 'Contrast2012', 'Highlights2012', 'Shadows2012', 'Whites2012',
            'Blacks2012', 'Texture', 'Clarity2012', 'Dehaze', 'Vibrance', 'Saturation', 'Sharpness',
            'LuminanceSmoothing', 'ColorNoiseReduction', 'PostCropVignetteAmount', 'CropAngle',
            'ColorGradeMidtoneHue', 'ColorGradeMidtoneSat', 'ColorGradeHighlightHue', 'ColorGradeHighlightSat',
            'ColorGradeShadowHue', 'ColorGradeShadowSat', 'SaturationAdjustmentOrange', 'LuminanceAdjustmentOrange',
            'HueAdjustmentOrange', 'SaturationAdjustmentRed', 'SaturationAdjustmentYellow', 'SaturationAdjustmentGreen',
            'HueAdjustmentGreen', 'LuminanceAdjustmentGreen', 'SaturationAdjustmentBlue']
    print(f"\n{'setting':32} {'used%':>6} {'p10':>8} {'median':>8} {'p90':>8}")
    for k in keys:
        vals = [d['s'][k] for d in edited if isinstance(d['s'].get(k), (int, float))]
        nz = [v for v in vals if v != 0]
        if not vals:
            continue
        print(f'{k:32} {100 * len(nz) / len(edited):6.0f} {q(vals, .1):8.2f} {q(vals, .5):8.2f} {q(vals, .9):8.2f}')
    for k in ['MaskGroupBasedCorrectionsCount', 'RetouchInfoCount', 'PaintBasedCorrectionsCount', 'GradientBasedCorrectionsCount']:
        n = sum(1 for d in edited if d['s'].get(k))
        print(f'{k:32} on {100 * n / max(1, len(edited)):.0f}% of developed photos')

    def light(d):
        iso = d['iso'] or 0
        if d['flash'] and d['flash'] > 0:
            return 'flash'
        return 'ISO<=400 (bright/day)' if iso <= 400 else 'ISO 401-1600 (indoor/shade)' if iso <= 1600 else 'ISO>1600 (dark/night)'

    print('\nBy light (ISO and flash as the measure of how much light there was):')
    groups = defaultdict(list)
    for d in edited:
        groups[light(d)].append(d)
    for g, ds in sorted(groups.items()):
        def med(k):
            v = [d['s'][k] for d in ds if isinstance(d['s'].get(k), (int, float))]
            return statistics.median(v) if v else float('nan')
        wb = Counter(d['wb_mode'] for d in ds).most_common(2)
        print(f'  {g:30} n={len(ds):5} temp {med("Temperature"):6.0f} tint {med("Tint"):+4.0f} exp {med("Exposure2012"):+5.2f} '
              f'contr {med("Contrast2012"):+4.0f} hi {med("Highlights2012"):+4.0f} sh {med("Shadows2012"):+4.0f} '
              f'wh {med("Whites2012"):+4.0f} bl {med("Blacks2012"):+4.0f} vib {med("Vibrance"):+4.0f}  wb {wb}')
    print('\nBy shoot:')
    shoots = defaultdict(list)
    for d in edited:
        shoots[d['shoot']].append(d)
    for g, ds in sorted(shoots.items(), key=lambda kv: -len(kv[1])):
        def med(k):
            v = [d['s'][k] for d in ds if isinstance(d['s'].get(k), (int, float))]
            return statistics.median(v) if v else float('nan')
        print(f'  {g[:34]:34} n={len(ds):5} temp {med("Temperature"):6.0f} tint {med("Tint"):+4.0f} exp {med("Exposure2012"):+5.2f} '
              f'hi {med("Highlights2012"):+4.0f} sh {med("Shadows2012"):+4.0f} wh {med("Whites2012"):+4.0f} bl {med("Blacks2012"):+4.0f} '
              f'look {Counter(d["s"].get("LookName") for d in ds).most_common(1)[0][0]}')


if __name__ == '__main__':
    out, cats = sys.argv[1], sys.argv[2:]
    data = [r for cat in cats for r in rows(cat)]
    json.dump(data, open(out, 'w', encoding='utf-8'))
    summary(data)
