"""Independently verify saved desktop match pixels against the unedited originals.

Run after test-main-match-twenty.py. --require-changes exits unsuccessfully if
any attempted match was refused, kept the original, or left its reference-stage
baseline unchanged. The report is written even when this qualification fails.
"""
import argparse
import csv
import json
from pathlib import Path

import numpy as np
from PIL import Image


def verify(run):
    data = json.loads((run / 'results.json').read_text(encoding='utf-8'))
    if not data.get('completed') or not all(data.get('checks', {}).values()):
        raise ValueError('Complete the desktop run and its integrity checks first.')
    rows = data['pairs']
    if len(rows) != data['targetCount'] * len(data['workflows']):
        raise ValueError('The run is missing requested attempts.')
    for row in rows:
        with Image.open(run / f'{row["targetIndex"]:02}.original.png') as original:
            original = np.asarray(original.convert('RGB')).astype(np.int16)
        with Image.open(run / f'{row["stem"]}.matched.png') as saved:
            saved = np.asarray(saved.convert('RGB')).astype(np.int16)
        if original.shape != saved.shape:
            raise ValueError(f'Output dimensions changed: {row["stem"]}')
        identical = bool(np.array_equal(original, saved))
        mean_rgb = float(np.abs(original - saved).mean())
        fraction = float(np.any(original != saved, axis=2).mean())
        if (identical != row['originalPixelIdentical']
                # The desktop harness accumulates this mean in float32; this
                # independent integer/float64 calculation retains more precision.
                or not np.isclose(mean_rgb, row['originalMeanRgbChange'], rtol=2e-6, atol=1e-6)
                or abs(fraction - row['originalChangedPixelFraction']) > 1e-9):
            raise ValueError(f'Saved pixel evidence disagrees with measurements: {row["stem"]}')
    summary = []
    for workflow in data['workflows']:
        subset = [row for row in rows if row['workflow'] == workflow]
        summary.append(dict(
            workflow=workflow, attempts=len(subset),
            originalDifferent=sum(not row['originalPixelIdentical'] for row in subset),
            materiallyChanged=sum(row['originalMeanRgbChange'] >= .5
                                  and row['originalMeanDeltaE76'] >= .5 for row in subset),
            referenceStageChanged=sum(row['meanRgbChange'] > 0 for row in subset),
            refused=sum('error' in row for row in subset),
            acceptedUnchangedReferenceStage=sum('error' not in row and row['meanRgbChange'] == 0
                                              for row in subset),
            smallestMeanRgbChange=min(row['originalMeanRgbChange'] for row in subset),
            smallestChangedPixelFraction=min(row['originalChangedPixelFraction'] for row in subset),
            identicalTargets=[row['targetIndex'] for row in subset if row['originalPixelIdentical']],
            weakTargets=[row['targetIndex'] for row in subset
                         if not row['originalPixelIdentical']
                         and (row['originalMeanRgbChange'] < .5 or row['originalMeanDeltaE76'] < .5)]))
    requirements = dict(
        allFinalOutputsDifferFromOriginal=all(not row['originalPixelIdentical'] for row in rows),
        everyReferenceStageChangesPixels=all(row['meanRgbChange'] > 0 for row in rows),
        allRequestedMatchesSaved=all('error' not in row for row in rows))
    failed = [dict(target=row['targetIndex'], workflow=row['workflow'], style=row['style'],
                   originalIdentical=row['originalPixelIdentical'],
                   referenceStageUnchanged=row['meanRgbChange'] == 0, error=row.get('error'))
              for row in rows if row['originalPixelIdentical'] or row['meanRgbChange'] == 0 or 'error' in row]
    output = dict(
        exeSha256=data['exeSha256'], run=str(run), summary=summary,
        savedOriginalPixelEvidenceVerified=True, requirements=requirements,
        qualificationPassed=all(requirements.values()), failedCases=failed,
        materialChangeRule='Authored diagnostic floor: mean RGB8 change >=0.5 and mean CIE76 >=0.5 against the unedited native original. This is not a perceptual-style guarantee.',
        automaticCorrectionCaveat='Collection can differ from original through automatic correction even when its reference stage is unchanged.')
    (run / 'original-output-verification.json').write_text(json.dumps(output, indent=2), encoding='utf-8')
    with (run / 'original-output-verification.csv').open('w', encoding='utf-8-sig', newline='') as stream:
        names = ['workflow', 'targetIndex', 'description', 'style', 'originalPixelIdentical',
                 'originalMeanRgbChange', 'originalMeanDeltaE76', 'originalP95DeltaE76',
                 'originalMeanToneChange', 'originalMeanChromaChange', 'originalChangedPixelFraction',
                 'meanRgbChange', 'verdict', 'error']
        writer = csv.DictWriter(stream, fieldnames=names, extrasaction='ignore')
        writer.writeheader()
        writer.writerows(rows)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('--require-changes', action='store_true')
    args = parser.parse_args()
    result = verify(args.run.resolve())
    print(json.dumps(result, indent=2))
    return 1 if args.require_changes and not result['qualificationPassed'] else 0


if __name__ == '__main__':
    raise SystemExit(main())
