"""Compare two completed, identical-input desktop palette assessments.

Shadow steps are measured on every eligible neighbouring pair, independently of
the native fitter's stratified sample. The authored RGB8 thresholds are a proxy
for contouring, not a universal perceptual quality or colour-accuracy score.
"""
import argparse
import csv
import json
from pathlib import Path
import statistics

import numpy as np
from PIL import Image, ImageDraw, ImageFont


def pixels(path):
    with Image.open(path) as image:
        return np.array(image.convert('RGB'), dtype=np.int16)


def mask_pairs(source):
    masks = []
    for left, right in ((source[:, :-1], source[:, 1:]), (source[:-1], source[1:])):
        masks.append((left.max(axis=2) <= 96) & (right.max(axis=2) <= 96)
                     & (np.abs(left - right).max(axis=2) <= 2))
    return masks


def steps(image, masks):
    values = np.concatenate([
        np.abs(left - right).max(axis=2)[mask]
        for (left, right), mask in zip(((image[:, :-1], image[:, 1:]), (image[:-1], image[1:])), masks)
    ])
    return dict(pairs=int(values.size), largeSteps=int((values >= 12).sum()),
                fraction=float((values >= 12).mean()) if values.size else None,
                p95=float(np.quantile(values, .95)) if values.size else None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('updated', type=Path)
    args = parser.parse_args()
    baseline, updated = args.baseline.resolve(), args.updated.resolve()
    reports = [json.loads((root / 'results.json').read_text(encoding='utf-8')) for root in (baseline, updated)]
    for report in reports:
        assert not report.get('failure') and all(report['checks'].values()), 'Require completed desktop runs'
        assert report['intent'] == 'palette'
    indexed = [{(row['targetIndex'], row['style']): row for row in report['pairs']} for report in reports]
    independent_path = updated / 'independent-analysis.json'
    independent = ({(row['targetIndex'], row['style']): row for row in json.loads(independent_path.read_text(encoding='utf-8'))['pairs']}
                   if independent_path.exists() else {})
    assert indexed[0].keys() == indexed[1].keys(), 'Pair sets differ'
    for reference in reports[0]['references']:
        style = reference['style']
        assert np.array_equal(pixels(baseline / 'references' / f'{style}.png'), pixels(updated / 'references' / f'{style}.png'))
    originals = {}
    rows = []
    for key, old in indexed[0].items():
        new = indexed[1][key]
        assert (old['photoId'], old['referencePhotoId']) == (new['photoId'], new['referencePhotoId'])
        target, style = key
        if target not in originals:
            source = pixels(baseline / f'{target:02}.before.png')
            assert np.array_equal(source, pixels(updated / f'{target:02}.before.png')), 'Native baseline pixels differ'
            originals[target] = mask_pairs(source)
        paths = [root / f'{row["stem"]}.matched.png' for root, row in ((baseline, old), (updated, new))]
        old_image, new_image = [pixels(path) for path in paths]
        old_steps, new_steps = [steps(image, originals[target]) for image in (old_image, new_image)]
        transfer = new.get('native', {}).get('fitting', {}).get('transfer')
        row = dict(targetIndex=target, photoId=new['photoId'], referencePhotoId=new['referencePhotoId'], style=style,
                   baselinePaletteDistance=old['paletteAfter']['color'], updatedPaletteDistance=new['paletteAfter']['color'],
                   baselineToneDistance=old['paletteAfter']['tone'], updatedToneDistance=new['paletteAfter']['tone'],
                   baselineOracleMean=old['oracle']['meanDeltaE76'], updatedOracleMean=new['oracle']['meanDeltaE76'],
                   updatedOracleP95=new['oracle']['p95DeltaE76'], exactKnownEdit=new['oracle']['bitwiseIdentical'],
                   closeKnownEdit=new['oracle']['closeKnownLook'],
                   closeReferencePaletteAndTone=independent.get(key, {}).get('closeReferencePaletteAndTone'),
                   updatedJointColorDistance=independent.get(key, {}).get('jointColorDistanceAfter'),
                   updatedVerdict=new['verdict'],
                   nativePaletteNonRegressing=(transfer['renderedAfter']['palette'] <= transfer['renderedBefore']['palette'] + .0001) if transfer else None,
                   nativeTotalNonRegressing=(transfer['renderedAfter']['total'] <= transfer['renderedBefore']['total'] + .0001) if transfer else None,
                   baselineLargeStepFraction=old_steps['fraction'], updatedLargeStepFraction=new_steps['fraction'],
                   eligibleShadowPairs=new_steps['pairs'], baselineLargeStepCount=old_steps['largeSteps'],
                   updatedLargeStepCount=new_steps['largeSteps'], baselineStepP95=old_steps['p95'], updatedStepP95=new_steps['p95'],
                   identicalOutput=bool(np.array_equal(old_image, new_image)),
                   updatedRefused=bool(new.get('error')))
        rows.append(row)
    eligible = [row for row in rows if row['eligibleShadowPairs'] >= 128]
    summary = dict(baseline=str(baseline), updated=str(updated), pairs=len(rows),
                   baselinePixelsIdentical=True, referencePixelsIdentical=True,
                   changedOutputs=sum(not row['identicalOutput'] for row in rows),
                   nativePaletteRegressions=sum(row['nativePaletteNonRegressing'] is False for row in rows),
                   nativeTotalRegressions=sum(row['nativeTotalNonRegressing'] is False for row in rows),
                   paletteCloserThanBaseline=sum(row['updatedPaletteDistance'] < row['baselinePaletteDistance'] - .001 for row in rows),
                   paletteFartherThanBaseline=sum(row['updatedPaletteDistance'] > row['baselinePaletteDistance'] + .001 for row in rows),
                   knownEditCloserThanBaseline=sum(row['updatedOracleMean'] < row['baselineOracleMean'] - .001 for row in rows),
                   knownEditFartherThanBaseline=sum(row['updatedOracleMean'] > row['baselineOracleMean'] + .001 for row in rows),
                   eligibleShadowComparisons=len(eligible),
                   fewerLargeShadowSteps=sum(row['updatedLargeStepCount'] < row['baselineLargeStepCount'] for row in eligible),
                   moreLargeShadowSteps=sum(row['updatedLargeStepCount'] > row['baselineLargeStepCount'] for row in eligible),
                   baselineTotalLargeSteps=sum(row['baselineLargeStepCount'] for row in eligible),
                   updatedTotalLargeSteps=sum(row['updatedLargeStepCount'] for row in eligible),
                   baselineAnalysisMedianMs=statistics.median(row['analysisMilliseconds'] for row in reports[0]['pairs'] if 'analysisMilliseconds' in row),
                   updatedAnalysisMedianMs=statistics.median(row['analysisMilliseconds'] for row in reports[1]['pairs'] if 'analysisMilliseconds' in row),
                   baselineApplyMedianMs=statistics.median(row['applyMilliseconds'] for row in reports[0]['pairs'] if 'applyMilliseconds' in row),
                   updatedApplyMedianMs=statistics.median(row['applyMilliseconds'] for row in reports[1]['pairs'] if 'applyMilliseconds' in row),
                   meaning='Same baseline/reference pixels. Full-frame smooth-shadow step proxy; thresholds are authored quality limits, not proof of perfect colour.')
    (updated / 'upgrade-comparison.json').write_text(json.dumps(dict(summary=summary, pairs=rows), indent=2), encoding='utf-8')
    with (updated / 'upgrade-comparison.csv').open('w', newline='', encoding='utf-8') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(sorted(rows, key=lambda row: (row['targetIndex'], row['style'])))
    if (17, 'teal-orange') in indexed[0]:
        sheet = Image.new('RGB', (1500, 570), '#17171c')
        draw = ImageDraw.Draw(sheet)
        font = ImageFont.truetype('C:/Windows/Fonts/arial.ttf', 16)
        paths = [updated / '17.before.png', updated / 'references/teal-orange.png',
                 baseline / '17.teal-orange.matched.png', updated / '17.teal-orange.matched.png',
                 updated / '17.teal-orange.expected.png']
        labels = ['Original target', 'Different reference', 'Previous native v5', 'Updated native v6', 'Known expected edit']
        for col, (path, label) in enumerate(zip(paths, labels)):
            with Image.open(path) as source:
                photo = source.convert('RGB')
            photo.thumbnail((290, 520))
            sheet.paste(photo, (col * 300 + (300 - photo.width) // 2, 40))
            draw.text((col * 300 + 8, 10), label, fill='white', font=font)
        sheet.save(updated / 'shadow-regression-comparison.jpg', quality=95)
    print(json.dumps(summary, indent=2), flush=True)


if __name__ == '__main__':
    main()
