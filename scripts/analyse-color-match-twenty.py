"""Independently measure completed desktop assessment pixels and write pair tables.

No native scores or acceptance criteria are changed. The joint colour metric uses
32 deterministic CIE a*b* projections of 2048 stratified samples, unlike the
matcher's OKLab fit. CIE76 is reported as a distance, never a percent accuracy.
"""
import argparse
from collections import Counter
import importlib.util
import json
from pathlib import Path
import sqlite3
import statistics

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('assessment', ROOT / 'scripts/test-color-match-twenty.py')
assessment = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assessment)


def joint_samples(image):
    pixels = assessment.lab(image).reshape(-1, 3)[:, 1:]
    indices = ((np.arange(2048) + .5) * len(pixels) / 2048).astype(int).clip(0, len(pixels) - 1)
    angle = np.arange(32) * 2.399963229728653
    directions = np.stack([np.cos(angle), np.sin(angle)])
    return np.sort(pixels[indices] @ directions, axis=0)


def diagnose(row):
    reasons = []
    if row['paletteBefore']['color'] <= .1 and row['paletteAfter']['color'] <= .1:
        reasons.append('chroma already neutral; relative colour percentages are below the declared 0.1-unit measurement floor')
    if row.get('error'):
        reasons.append('native fit refused')
    if row['colorImprovementPercent'] < 0:
        reasons.append('reference colour quantiles worsened')
    if row['oracleImprovementPercent'] < 0:
        reasons.append('farther from known edit than unedited baseline')
    if row.get('native', {}).get('fitting', {}).get('nativeBacktrackFactor', 1) < .5:
        reasons.append('native checks substantially reduced fitted strength')
    if not row['oracle']['closeKnownLook']:
        e = row['oracle']
        reasons.append('tone differs' if e['meanAbsToneError'] > e['meanChromaPlaneError'] else 'hue/chroma/cast differs')
        dl, da, db = e['meanSignedLabError']
        if abs(dl) > 3:
            reasons.append('brighter than known edit' if dl > 0 else 'darker than known edit')
        if abs(da) > 2:
            reasons.append('red-axis shift from known edit' if da > 0 else 'green-axis shift from known edit')
        if abs(db) > 2:
            reasons.append('yellow-axis shift from known edit' if db > 0 else 'blue-axis shift from known edit')
        zones = [(e[name]['meanDeltaE76'], name) for name in ('shadows', 'midtones', 'highlights') if e[name]['pixels'] >= 100]
        if zones:
            reasons.append('largest residual in ' + max(zones)[1])
    if not reasons:
        reasons.append('close under declared thresholds; residual differences remain' if not row['oracle']['bitwiseIdentical'] else 'exact known edit')
    return reasons


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    args = parser.parse_args()
    out = args.run.resolve()
    data = json.loads((out / 'results.json').read_text(encoding='utf-8'))
    assert not data.get('failure') and data.get('checks', {}).get('allPairsRun'), 'Only analyse a completed run'
    refs = {x['style']: joint_samples(Image.open(x['path']).convert('RGB')) for x in data['references']}
    before_samples = {}
    rows = []
    calibrated_amounts = {}
    if data['intent'] == 'paired-calibration':
        uri = (out / 'catalog.sqlite').as_uri() + '?mode=ro'
        with sqlite3.connect(uri, uri=True) as catalog:
            for photo, label, body in catalog.execute("SELECT photo_id,label,body FROM edit_history WHERE label LIKE 'Reference style:%' ORDER BY seq"):
                recipe = json.loads(body)
                for style in assessment.STYLES:
                    if label.startswith((f'Reference style: {style} (', f'Reference style: @{style} (')):
                        calibrated_amounts[(photo, style)] = recipe.get('calibrated_look_v1', {}).get('amount', 0)
    for row in data['pairs']:
        key = row['targetIndex']
        if key not in before_samples:
            before_samples[key] = joint_samples(Image.open(out / f'{key:02}.before.png').convert('RGB'))
        actual = Image.open(out / f'{row["stem"]}.matched.png').convert('RGB')
        joint_before = float(np.abs(before_samples[key] - refs[row['style']]).mean())
        joint_after = float(np.abs(joint_samples(actual) - refs[row['style']]).mean())
        rows.append(dict(targetIndex=key, photoId=row['photoId'], style=row['style'], verdict=row['verdict'],
                         paletteBefore=row['paletteBefore'], paletteAfter=row['paletteAfter'],
                         colorImprovementPercent=row['colorImprovementPercent'],
                         jointColorDistanceBefore=joint_before, jointColorDistanceAfter=joint_after,
                         jointColorImprovementPercent=assessment.helpers.reduction(joint_before, joint_after),
                         meaningfulColorSignal=row['paletteBefore']['color'] > .1,
                         meaningfulJointColorSignal=joint_before > .1,
                         closeReferencePaletteAndTone=row['paletteAfter']['color'] <= 3 and row['paletteAfter']['tone'] <= 3 and joint_after <= 2,
                         oracle=row['oracle'], oracleBefore=row['oracleBefore'], oracleImprovementPercent=row['oracleImprovementPercent'],
                         backtrackFactor=row.get('native', {}).get('fitting', {}).get('nativeBacktrackFactor'),
                         analysisMilliseconds=row.get('analysisMilliseconds'), applyMilliseconds=row.get('applyMilliseconds'),
                         reasons=diagnose(row)))
        if data['intent'] == 'paired-calibration':
            rows[-1]['calibratedAmount'] = calibrated_amounts.get((row['auraPhotoId'], row['style']), 0)
            if rows[-1]['calibratedAmount'] < 1:
                rows[-1]['reasons'].append(f'calibrated grade attenuated to {rows[-1]["calibratedAmount"]:.0%}')
    summaries = []
    for style in assessment.STYLES:
        subset = [x for x in rows if x['style'] == style]
        if not subset:
            continue
        summaries.append(dict(style=style, pairs=len(subset), exact=sum(x['oracle']['bitwiseIdentical'] for x in subset),
                              close=sum(x['oracle']['closeKnownLook'] for x in subset),
                              colorQuantilesImproved=sum(x['colorImprovementPercent'] > 0 for x in subset),
                              jointColorImproved=sum(x['jointColorImprovementPercent'] > 0 for x in subset),
                              meaningfulColorComparisons=sum(x['meaningfulColorSignal'] for x in subset),
                              meaningfulColorImproved=sum(x['meaningfulColorSignal'] and x['colorImprovementPercent'] > 0 for x in subset),
                              knownEditImproved=sum(x['oracleImprovementPercent'] > 0 for x in subset),
                              medianColorImprovementPercent=statistics.median(x['colorImprovementPercent'] for x in subset),
                              medianJointColorImprovementPercent=statistics.median(x['jointColorImprovementPercent'] for x in subset),
                              medianOracleDeltaE76=statistics.median(x['oracle']['meanDeltaE76'] for x in subset)))
    summary = dict(intent=data['intent'], run=str(out), pairs=len(rows),
                   exact=sum(x['oracle']['bitwiseIdentical'] for x in rows), close=sum(x['oracle']['closeKnownLook'] for x in rows),
                   closeReferencePaletteAndTone=sum(x['closeReferencePaletteAndTone'] for x in rows),
                   colorQuantilesImproved=sum(x['colorImprovementPercent'] > 0 for x in rows),
                   jointColorImproved=sum(x['jointColorImprovementPercent'] > 0 for x in rows),
                   meaningfulColorComparisons=sum(x['meaningfulColorSignal'] for x in rows),
                   meaningfulColorImproved=sum(x['meaningfulColorSignal'] and x['colorImprovementPercent'] > 0 for x in rows),
                   meaningfulJointColorComparisons=sum(x['meaningfulJointColorSignal'] for x in rows),
                   meaningfulJointColorImproved=sum(x['meaningfulJointColorSignal'] and x['jointColorImprovementPercent'] > 0 for x in rows),
                   knownEditImproved=sum(x['oracleImprovementPercent'] > 0 for x in rows),
                   verdicts=dict(Counter(x['verdict'] for x in rows)), styles=summaries,
                   medianAnalysisMilliseconds=statistics.median(x['analysisMilliseconds'] for x in rows if x['analysisMilliseconds'] is not None) if any(x['analysisMilliseconds'] is not None for x in rows) else None,
                   medianApplyMilliseconds=statistics.median(x['applyMilliseconds'] for x in rows if x['applyMilliseconds'] is not None))
    (out / 'independent-analysis.json').write_text(json.dumps(dict(summary=summary, pairs=rows), indent=2), encoding='utf-8')
    lines = ['# Independent pair measurements', '', 'Distances are CIE76 and CIE a*b* distribution distances, not percentages of accuracy. Positive improvement means closer; negative means worse. Exact means bitwise identical target/oracle pixels.', '',
             '| Target | Grade | Reference colour distance before → after | Joint colour improvement | Known-edit mean / p95 ΔE76 | Exact? | Diagnosis |',
             '|---|---|---|---|---|---|---|']
    for x in sorted(rows, key=lambda x: (x['targetIndex'], x['style'])):
        lines.append(f'| {x["targetIndex"]:02} ({x["photoId"]}) | {x["style"]} | {x["paletteBefore"]["color"]:.2f} → {x["paletteAfter"]["color"]:.2f} | {x["jointColorImprovementPercent"]:+.1f}% | {x["oracle"]["meanDeltaE76"]:.2f} / {x["oracle"]["p95DeltaE76"]:.2f} | {"Yes" if x["oracle"]["bitwiseIdentical"] else "No"} | {"; ".join(x["reasons"])} |')
    (out / 'pair-measurements.md').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    # Review original, separate reference, expected edit, actual output for selected
    # diverse scenes. Full-resolution individual files remain linked in index.html.
    font = ImageFont.truetype('C:/Windows/Fonts/arial.ttf', 16)
    for style in assessment.STYLES:
        subset = [x for x in data['pairs'] if x['style'] == style]
        if not subset:
            continue
        sheet = Image.new('RGB', (1280, 70 + len(subset) * 260), '#17171c')
        draw = ImageDraw.Draw(sheet)
        draw.text((12, 12), f'{style}: Original target | Different reference | Known expected edit | Actual AURA', font=font, fill='white')
        for i, row in enumerate(subset):
            y = 70 + i * 260
            paths = [out / f'{row["targetIndex"]:02}.before.png', out / 'references' / f'{style}.png', out / f'{row["stem"]}.expected.png', out / f'{row["stem"]}.matched.png']
            for col, path in enumerate(paths):
                photo = Image.open(path).convert('RGB')
                photo.thumbnail((304, 215))
                sheet.paste(photo, (col * 320 + (320 - photo.width) // 2, y))
            draw.text((12, y + 220), f'{row["targetIndex"]:02}  {row["verdict"]}; known-edit mean DeltaE76 {row["oracle"]["meanDeltaE76"]:.2f}', font=font, fill='#ffe1ad')
        sheet.save(out / f'{style}.comparison.jpg', quality=92)
    print(json.dumps(summary, indent=2), flush=True)


if __name__ == '__main__':
    main()
