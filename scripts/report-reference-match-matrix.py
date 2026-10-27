"""Summarize a complete desktop colour-match matrix without changing its results."""
import argparse
import importlib.util
import json
from pathlib import Path
import statistics

import numpy as np
from PIL import Image, ImageDraw, ImageOps

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('matrix_test', ROOT / 'scripts/test-reference-match-matrix.py')
matrix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matrix)
TYPES = {
    1: ('Portrait / night', 'Night portrait with flash'), 2: ('Night city', 'Illuminated waterfront skyline'),
    3: ('Architecture', 'Cafe, brick and red flowers'), 4: ('Portrait / night', 'Person on beach at night'),
    5: ('Landscape', 'Beach at dusk / backlight'), 6: ('Flowers / bright background', 'Red flowers in snow'),
    7: ('Landscape', 'Mossy forest waterfall'), 8: ('Landscape', 'Autumn forest waterfall'),
    9: ('Landscape', 'Desert and smooth sky'), 10: ('Portrait / daylight', 'Older man outdoors'),
    11: ('Portrait / dramatic light', 'Bearded man, high contrast'), 12: ('Portrait / soft light', 'Older man, close soft-lit portrait'),
    13: ('Portrait / monochrome', 'Already black-and-white portrait'), 14: ('Food', 'Bright fruit close-up'),
    15: ('Food', 'Fruit in low light / bokeh'), 16: ('Portrait / studio', 'White corset, warm studio light'),
    17: ('Portrait / studio', 'Dim studio and dark curtain'), 18: ('Portrait / dramatic light', 'Red studio illumination'),
    19: ('Portrait / studio', 'Studio portrait with natural hair'), 20: ('Portrait / studio', 'Pastel dress and pale floor'),
}


def counts(rows):
    result = matrix.summarize(rows)
    result['newlyCloseChanged'] = sum(x['oracle']['closeKnownLook'] and not x['oracleBefore']['closeKnownLook'] and x['meanRgbChange'] > 0 and 'error' not in x for x in rows)
    return result


def cell(row):
    return f'{row["targetIndex"]:02}/{row["workflow"]}/{row["style"]}'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    args = parser.parse_args()
    out = args.run.resolve()
    report = json.loads((out / 'results.json').read_text())
    assert report.get('completed') and all(report['checks'].values()), 'Complete the matrix before classifying it'
    rows = report['pairs']
    summary = dict(workflows=[], styles=[], categories=[], photographs=[], gradientDiagnostics=[])
    lines = ['## Full matrix results', '', 'All counts below use the completed 360-case matrix at 80% requested strength. Close known results use the declared mean/p95 QA thresholds and the interpolation caveat explained in the report. Changed-close excludes refused and unchanged results.', '',
             '| Workflow | Attempts | Refused | Changed | Reference colour improved | Joint colour improved | Close reference colour + tone | Known edit closer | Close known / changed-close / newly close | Exact known | Median known error |',
             '|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|']
    for workflow in matrix.WORKFLOWS:
        stat = dict(workflow=workflow, **counts([x for x in rows if x['workflow'] == workflow]))
        summary['workflows'].append(stat)
        lines.append(f'| {workflow} | {stat["attempted"]} | {stat["refused"]} | {stat["changed"]} | {stat["referenceColorImproved"]} | {stat["jointColorImproved"]} | {stat["closeReferenceColorTone"]} | {stat["knownEditImproved"]} | {stat["closeKnownEdit"]} / {stat["closeKnownChanged"]} / {stat["newlyCloseChanged"]} | {stat["exactKnownEdit"]} | {stat["medianKnownEditError"]:.3f} |')
    lines.extend(['', 'Newly close requires a changed accepted result that passes the known-edit thresholds when its own baseline did not. A result that was already close before matching is not counted as newly successful.', '', '### Results by requested look', '', '| Look | Workflow | Refused / 20 | Close reference colour + tone / 20 | Known edit closer / 20 | Changed-close / newly close known | Median known error |', '|---|---|---:|---:|---:|---:|---:|'])
    for style in matrix.STYLES:
        for workflow in matrix.WORKFLOWS:
            stat = dict(style=style, workflow=workflow, **counts([x for x in rows if x['workflow'] == workflow and x['style'] == style]))
            summary['styles'].append(stat)
            lines.append(f'| {style} | {workflow} | {stat["refused"]} | {stat["closeReferenceColorTone"]} | {stat["knownEditImproved"]} | {stat["closeKnownChanged"]} / {stat["newlyCloseChanged"]} | {stat["medianKnownEditError"]:.3f} |')
    lines.extend(['', '### Results by photo type', '', 'Repeated references/modes on one photograph are correlated observations. A category containing one or two photographs does not establish a general capability for that category.', '',
                  '| Type | Distinct photos | Attempts | Refused | Reference colour improved | Changed-close / newly close known | Median known error |', '|---|---:|---:|---:|---:|---:|---:|'])
    for category in sorted({x[0] for x in TYPES.values()}):
        subset = [x for x in rows if TYPES[x['targetIndex']][0] == category]
        stat = dict(category=category, distinctPhotos=len({x['targetIndex'] for x in subset}), **counts(subset))
        summary['categories'].append(stat)
        lines.append(f'| {category} | {stat["distinctPhotos"]} | {stat["attempted"]} | {stat["refused"]} | {stat["referenceColorImproved"]} | {stat["closeKnownChanged"]} / {stat["newlyCloseChanged"]} | {stat["medianKnownEditError"]:.3f} |')
    lines.extend(['', '### Every tested photograph: successful and unsuccessful combinations', '', 'Each photo has 18 combinations. A listed changed-close case is a measured candidate, not a certification of all local details. “No changed-close” means none of the six tested looks and three modes passed that criterion on this photo, not that every possible future reference must fail.', '',
                  '| Photo | Type / subject | Changed-close / 18 | Best changed accepted case: mean / p95 error | Worst accepted changed case: mean error |', '|---:|---|---:|---|---|'])
    for index, (category, description) in TYPES.items():
        subset = [x for x in rows if x['targetIndex'] == index]
        changed = [x for x in subset if 'error' not in x and x['meanRgbChange'] > 0]
        close = [x for x in changed if x['oracle']['closeKnownLook']]
        best = min(changed, key=lambda x: x['oracle']['meanDeltaE76']) if changed else None
        worst = max(changed, key=lambda x: x['oracle']['meanDeltaE76']) if changed else None
        summary['photographs'].append(dict(index=index, category=category, description=description, changedClose=len(close), best=cell(best) if best else None, worst=cell(worst) if worst else None,
                                           closeCases=[cell(x) for x in close], bestMean=best['oracle']['meanDeltaE76'] if best else None, worstMean=worst['oracle']['meanDeltaE76'] if worst else None))
        best_text = f'{best["workflow"]}/{best["style"]}: {best["oracle"]["meanDeltaE76"]:.3f} / {best["oracle"]["p95DeltaE76"]:.3f}' if best else 'None'
        worst_text = f'{worst["workflow"]}/{worst["style"]}: {worst["oracle"]["meanDeltaE76"]:.3f}' if worst else 'None'
        lines.append(f'| {index:02} | {description} | {len(close)} | {best_text} | {worst_text} |')
        # Contact sheet: references and three workflows, all six styles, plus actual baselines.
        sheet = Image.new('RGB', (1600, 1100), '#1d1d25'); draw = ImageDraw.Draw(sheet)
        draw.text((10, 8), f'{index:02}. {description} — six references and three native workflows', fill='white')
        for column, workflow in enumerate(['natural', 'collection']):
            row = next(x for x in subset if x['workflow'] == workflow)
            with Image.open(out / row['beforeFile']) as image:
                thumb = ImageOps.contain(image.convert('RGB'), (360, 200)); sheet.paste(thumb, (120 + column * 500, 35))
            draw.text((120 + column * 500, 241), f'{workflow} baseline', fill='white')
        for column, style in enumerate(matrix.STYLES):
            x = 120 + column * 245
            draw.text((x + 5, 271), style, fill='white')
            ref = next(r for r in report['references'] if r['style'] == style)
            for row_index, workflow in enumerate(['reference', *matrix.WORKFLOWS]):
                y = 295 + row_index * 195
                if column == 0: draw.text((5, y + 80), workflow, fill='white')
                row = next((r for r in subset if r['workflow'] == workflow and r['style'] == style), None)
                path = Path(ref['path']) if workflow == 'reference' else out / row['afterFile']
                with Image.open(path) as image:
                    thumb = ImageOps.contain(image.convert('RGB'), (239, 170)); sheet.paste(thumb, (x + (239 - thumb.width) // 2, y))
                if row:
                    label = f'{row["oracle"]["meanDeltaE76"]:.2f} / {row["verdict"]}'
                    draw.text((x + 2, y + 174), label, fill='#dddded')
        sheet.save(out / f'photo-{index:02}.jpg', quality=94)
    # Region-specific gradient diagnostics are distinct from the style-fidelity labels.
    for row in [x for x in rows if x['targetIndex'] == 9]:
        before = metrics_lab(out / row['beforeFile']); after = metrics_lab(out / row['afterFile'])
        h, w = before.shape[:2]; before = before[5:int(h * .45), 5:w-5]; after = after[5:int(h * .45), 5:w-5]
        a = steps(before); b = steps(after)
        summary['gradientDiagnostics'].append(dict(workflow=row['workflow'], style=row['style'], beforeP99=float(np.quantile(a, .99)), afterP99=float(np.quantile(b, .99)), beforeMax=float(a.max()), afterMax=float(b.max()), amplifiedPairs=int(((a <= 1) & (b > 3)).sum()), eligiblePairs=len(a)))
    lines.extend(['', '### Smooth-sky diagnostics across all reference looks', '', '| Workflow | Look | Adjacent-step p99 before / after | Maximum before / after | Pairs changing from ≤1 to >3 |', '|---|---|---:|---:|---:|'])
    for stat in summary['gradientDiagnostics']:
        lines.append(f'| {stat["workflow"]} | {stat["style"]} | {stat["beforeP99"]:.3f} / {stat["afterP99"]:.3f} | {stat["beforeMax"]:.3f} / {stat["afterMax"]:.3f} | {stat["amplifiedPairs"]} / {stat["eligiblePairs"]} |')
    summary['refusals'] = [{k: row.get(k) for k in ['targetIndex', 'workflow', 'style', 'error', 'refusalPreservedRecipe', 'uiProgressAfterRefusal']} for row in rows if 'error' in row]
    summary['closeChangedCases'] = [dict(case=cell(row), baselineAlreadyClose=row['oracleBefore']['closeKnownLook'], knownErrorImproved=row['oracle']['meanDeltaE76'] < row['oracleBefore']['meanDeltaE76']) for row in rows if row['oracle']['closeKnownLook'] and row['meanRgbChange'] > 0 and 'error' not in row]
    (out / 'analysis.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    (out / 'assessment-tables.md').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(json.dumps(dict(workflows=summary['workflows'], categories=summary['categories'], closeChangedCases=summary['closeChangedCases']), indent=2))


def metrics_lab(path):
    with Image.open(path) as image: return matrix.metrics.lab(image)


def steps(image):
    return np.concatenate([np.linalg.norm(np.diff(image, axis=0), axis=2).ravel(), np.linalg.norm(np.diff(image, axis=1), axis=2).ravel()])


if __name__ == '__main__':
    main()
