"""Compare the adaptive native matcher with retained baseline and prototype evidence."""
import csv
import hashlib
import html
import json
import os
from pathlib import Path
import shutil

from aura_colour_metrics import aligned_error, load_srgb

ROOT = Path(__file__).resolve().parents[1]
CHECKS = ROOT / '.work-checks/aura-adaptive-match-2026-10-22'
OUT = ROOT / 'output/aura-adaptive-match-2026-10-22'
STEM = ROOT / 'docs/reports/AURA-adaptive-colour-matching-2026-10-22'


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    old = read(ROOT / 'output/aura-reference-grades-retry-2026-10-22/results.json')
    prototype = read(ROOT / 'output/aura-adaptive-reference-grades-2026-10-22/results.json')
    matrix_root = ROOT / 'output/aura-adaptive-reference-final-2026-10-22'
    matrix = read(matrix_root / 'results.json')
    gui_root = ROOT / 'output/aura-adaptive-ui-2026-10-22'
    gui = read(gui_root / 'results.json')
    raw_root = Path('F:/AURA-verification/aura-adaptive-raw-2026-10-22')
    raw = read(raw_root / 'results.json')
    for run in [matrix, gui, raw]:
        assert run['completed'] and run['originalsUnchanged'] and not run['pageErrors']
    assert len(matrix['cases']) == 18 and len(gui['matches']) == 4
    assert matrix['sources'] == old['sources']
    assert matrix['executableSha256'] == gui['executableSha256'] == raw['executableSha256']
    indexed = lambda run: {(r['style'], r['target']): r for r in run['cases']}
    old_cases, prototype_cases = indexed(old), indexed(prototype)
    cases = []
    for row in matrix['cases']:
        key = row['style'], row['target']
        prior, proto = old_cases[key], prototype_cases[key]
        assert abs(prior['baselineToKnownGrade']['meanDeltaE76'] - row['baselineToKnownGrade']['meanDeltaE76']) < .00001
        previous = prior['matchedToKnownGrade']['meanDeltaE76']
        current = row['matchedToKnownGrade']['meanDeltaE76']
        cases.append(dict(style=row['style'], target=row['target'], sameImage=row['sameImage'],
            baselineMeanCie76=row['baselineToKnownGrade']['meanDeltaE76'], oldMatcherMeanCie76=previous,
            unregularizedPrototypeMeanCie76=proto['matchedToKnownGrade']['meanDeltaE76'], newMatcherMeanCie76=current,
            closerThanOld=current + .01 < previous, unchangedFromOld=abs(current-previous) <= .01,
            closerThanBaseline=current + .01 < row['baselineToKnownGrade']['meanDeltaE76'],
            exactPixels=row['matchedToKnownGrade']['exactPixels'], acceptedAmount=row['acceptedAmount'],
            fitting=row['analysis'].get('fitting'), referenceMonochrome=row['analysis'].get('referenceMonochrome')))
    exports = []
    for match in gui['matches']:
        name = Path(match['reference']).stem
        manifest = read(gui_root / ('export-' + name) / 'aura-delivery-manifest.json')
        index = next(i for i, p in enumerate(gui['photos']) if p['fileName'] == match['target'])
        path = gui_root / ('export-' + name) / manifest['files'][index]['path']
        actual, info = load_srgb(path)
        rendered, _ = load_srgb(gui_root / f'{name}-{match["target"]}-matched.png')
        error = aligned_error(actual, rendered)
        assert error['meanDeltaE76'] < 2 and error['p95DeltaE76'] < 5
        assert match['match']['afterDistance'] <= match['match']['beforeDistance'] + .0001
        exports.append(dict(reference=match['reference'], target=match['target'], file=str(path), info=info,
                            versusRender=error, match=match['match']))
    raw_jpeg = next(e for e in raw['exports'] if e['info']['format'] == 'JPEG')
    raw_png = next(e for e in raw['exports'] if e['info']['format'] == 'PNG')
    assert raw_png['versusRender']['exactPixels']
    delivered = OUT / '989A2865-adaptive-full.jpg'
    shutil.copy2(raw_jpeg['file'], delivered)
    assert hashlib.sha256(delivered.read_bytes()).hexdigest() == raw_jpeg['sha256']
    for image in ['before-preview.png', 'exported-jpeg-preview.png']:
        shutil.copy2(raw_root / image, OUT / ('raw-' + image))
    commands = {name: read(CHECKS / (name + '.json')) for name in ['native-source-final', 'ui-tests', 'ui-build-stable',
        'desktop-stable', 'controlled-final', 'desktop-gui', 'native-raw-export']}
    assert all(command['exitCode'] == 0 for command in commands.values())
    totals = dict(controlledPairs=18, closerThanOld=sum(c['closerThanOld'] for c in cases),
        unchangedFromOld=sum(c['unchangedFromOld'] for c in cases),
        closerThanBaseline=sum(c['closerThanBaseline'] for c in cases), pixelExact=sum(c['exactPixels'] for c in cases),
        guiSteps=len(gui['steps']), strengthChecks=len(gui['controls']),
        strengthsAccepted=sum(c['ok'] for c in gui['controls']), strengthsRefused=sum(not c['ok'] for c in gui['controls']),
        internetJpegs=4, nativeRawExports=2, universalExactMatchingEstablished=False)
    files = ['crates/aura-app/src/color_match.rs', 'crates/aura-app/src/color_match/refine.rs',
        'crates/aura-app/src/color_match/grade.rs', 'crates/aura-app/src/color_match/content.rs',
        'crates/aura-app/src/color_match/appearance.rs', 'crates/aura-app/src/color_match/library/workflows.rs',
        'crates/aura-app/tests/color_reference_match.rs', 'ui/src/ipc/colorMatch.ts']
    report = dict(totals=totals, cases=cases, internetExports=exports, rawExport=raw, commands=commands,
        executableSha256=gui['executableSha256'], prototypeExecutableSha256=prototype['executableSha256'],
        oldExecutableSha256=old['executableSha256'], sourcesSha256={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in files})
    STEM.with_suffix('.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    with Path(str(STEM) + '-cases.csv').open('w', encoding='utf-8', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=[k for k in cases[0] if k != 'fitting'])
        writer.writeheader()
        writer.writerows({k:v for k,v in c.items() if k != 'fitting'} for c in cases)
    table = '\n'.join(f"| {c['style']} / {c['target']} | {c['baselineMeanCie76']:.3f} | {c['oldMatcherMeanCie76']:.3f} | {c['unregularizedPrototypeMeanCie76']:.3f} | {c['newMatcherMeanCie76']:.3f} | {'closer' if c['closerThanOld'] else ('unchanged' if c['unchangedFromOld'] else 'farther')} |" for c in cases)
    internet_table = '\n'.join(f"| {e['reference']} / {e['target']} | {e['match']['beforeDistance']:.3f} → {e['match']['afterDistance']:.3f} | {e['versusRender']['meanDeltaE76']:.3f} |" for e in exports)
    text = f'''# AURA adaptive colour matcher — 2026-10-22

The native matching algorithm now searches signed corrections, separate curve anchors and colour bands, and tone-dependent casts. It handles newly imported achromatic references with a reversible grayscale contribution. Native-resolution validation checks the proposed treatment before analysis is saved; Apply rechecks the exact strengths. This is a working algorithm change, with the real desktop and exports tested.

**A universal 100% colour/tone match is not established.** In the repeated 18-case known-grade experiment, {totals['closerThanOld']}/18 results were closer than the old matcher, {totals['unchangedFromOld']}/18 were unchanged within 0.01 mean CIE76, {totals['closerThanBaseline']}/18 were closer than the ungraded baseline, and {totals['pixelExact']}/18 were pixel exact. Every row below is retained, including regressions. A correct implementation can still produce an imperfect photographic match; subject matter, lighting, clipping and protected edits constrain what a reference can tell it.

## What changed

- Additive pattern search explores both directions, including parameters initially zero, within the existing style bounds. It adjusts ten scalar controls, five curve anchors, and saturation/luminance for sufficiently supported colour bands. At most 160 refinement candidates are scored; duplicate canonical recipes reuse scores. Hue-band rotations remain conservative.
- A further 36 bounded probes fit independent shadow/midtone/highlight casts through the existing native grade wheels. Offsets use Cartesian hue/chroma coordinates to avoid the 0°/359° seam. Existing wheel luminance, global grade, blending and balance are retained, and manually protected fields pass through the same merge rules.
- A reference verified as achromatic from its imported pixels can remove chroma continuously through native saturation. Amount/Colour/Saturation strengths control that contribution, and zero restores the baseline. The achromatic skin guard checks every supported person, limits all seven tone landmarks and allows movement toward neutral while rejecting new casts and added chroma. Normal colour references keep the existing skin-colour guard. A selective-colour reference is not classified as monochrome merely because its 90th-percentile chroma is low.
- Before publishing a fit, render the native source and backtrack the fitted treatment through 100/75/50/25/0% if necessary to retain skin/highlights and avoid increasing native appearance distance. Apply remains explicit and can refuse unsafe custom strengths. The allowed native worsening tolerance was reduced from 0.25 to 0.0001 descriptor units.
- Keep old saved sessions readable. New imports/captures store bounded 32 × 32 RGB structure samples for gradient correlation; these are analysis data and are not copied into target pixels. Private reference metadata is upgraded when its source is reimported. Descriptor-only portable libraries retain their format and use conservative defaults until source pixels are supplied again.

## Regression found and corrected

The first unregularized prototype passed its functional tests while making some different-scene matches visibly washed out. For example, the warm known-grade error on target 1 rose from 7.903 to 21.933 even though its internal descriptor improved. That build was not qualified for release. Its complete results and binary hash are retained.

The revised search penalizes extra edit effort relative to the established guarded incumbent. Reference/target gradient correlation ≥0.95 permits a lighter 0.25 penalty; unrelated/unknown content uses 4.0. These are authored conservative policy values, not calibrated confidence probabilities or a guarantee. This reduces the opportunity to chase scene differences with large tone or cast changes. The final known-grade table below measures whether it helped.

## Controlled photographic results

The same three supplied-photo proxies and six known native AURA grades were used, with unchanged input hashes and matching baseline measurements. Compare against a known grade applied directly to each target. Mean aligned CIE76 is a difference measure, not a perfection percentage. The source-photo case tests reconstruction; the other two test transfer. No pixel comparison is assigned to unrelated Internet/reference scenes.

| Grade / target | Baseline error | Old matcher | Unregularized prototype | Revised matcher | Versus old |
|---|---:|---:|---:|---:|---|
{table}

[Full controlled gallery](../../output/aura-adaptive-reference-final-2026-10-22/index.html), [case CSV](AURA-adaptive-colour-matching-2026-10-22-cases.csv) and [machine report](AURA-adaptive-colour-matching-2026-10-22.json). Functional tests separately verify analysis does not edit history, matching is reversible, zero restores exact pixels and unsupported inputs are refused.

## Actual desktop and export checks

{totals['guiSteps']} desktop steps passed with both downloaded high-resolution references, four transfers, exact Undo/Redo and zero removal, reload, stale-analysis refusal and collection matching that preserves manual exposure. All four main and twelve fine controls were exercised in {totals['strengthChecks']} boundary cases: {totals['strengthsAccepted']} saved and {totals['strengthsRefused']} explicitly refused; refusals kept the saved recipe/history unchanged.

| Internet reference / target | Native appearance distance | Delivered JPEG mean CIE76 versus its render |
|---|---:|---:|
{internet_table}

The JPEGs were decoded with ICC conversion into sRGB and compared with the same target's saved export render. All four met the declared test bounds (mean CIE76 <2, p95 <5). Internal descriptor decrease is not proof of equal colour, tone, white balance or lighting across different subjects.

The original `H:\\AURA EDIT\\989A2865.CR3` was also matched and delivered at **{raw['nativeDimensions']['width']} × {raw['nativeDimensions']['height']}**. Native skin/highlight checks passed at amount {raw['match']['controls']['amount']:.2f}; appearance distance {raw['match']['beforeDistance']:.3f} → {raw['match']['afterDistance']:.3f}. PNG matches every export-render pixel. JPEG mean CIE76 {raw_jpeg['versusRender']['meanDeltaE76']:.4f}, p95 {raw_jpeg['versusRender']['p95DeltaE76']:.4f} on a declared 4-pixel sampling grid. Both recipe sidecars equal the delivered recipe; source/reference hashes stayed unchanged. Sidecars remain AURA canonical JSON with an `.xmp` suffix, without established Adobe/Evoto interoperability.

[Open the exported comparisons](../../output/aura-adaptive-match-2026-10-22/index.html) or the [full-resolution matched JPEG](../../output/aura-adaptive-match-2026-10-22/989A2865-adaptive-full.jpg). Native PNG/evidence remain in `{raw_root}`.

## Verification and limits

- Application library: 412 passed, 0 failed, 2 existing opt-in cases ignored. Reference integration: 5 passed, 0 failed, 1 opt-in RAW test ignored; the real RAW was exercised separately above.
- Frontend: 1052 tests passed in 111 files; final TypeScript/production UI build and desktop build passed. The existing non-default Windows manifest linker warning remains.
- Qualified desktop SHA-256: `{gui['executableSha256']}`. Exact commands, exit codes, source hashes and fitter diagnostics are in the machine report. Historical reports retain their own binary/test totals.
- New tests cover zero and wrong-sign seeds, independent curve/band/wheel fits, non-finite probes, evaluation budgets, cancellation, achromatic person guards, selective-colour detection, structure evidence and protected saturation. Earlier compile attempts caught a test Vec API mistake and a missing float type annotation; corrected attempts are retained rather than counted as passes.
- Distribution matching has a long research history; see [Reinhard et al.](https://home.cis.rit.edu/~cnspci/references/dip/color_transfer/reinhard2001.pdf) and [Pitié et al.](https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Pitie07.pdf). This implementation uses AURA's bounded parametric renderer and measured acceptance checks. It does not recover an arbitrary hidden LUT, original camera WB or scene illumination from a finished photograph.
- JPEG/opaque PNG/TIFF reference limits remain. Guard refusals, conservative no-change fits and imperfect matches remain possible. Exact Evoto equivalence and universal photographic correctness remain unverified.
'''
    STEM.with_suffix('.md').write_text(text, encoding='utf-8')
    cards = []
    for style, target in [('monochrome', '0.png'), ('monochrome', '1.png'), ('monochrome', '2.png'), ('split-tone', '0.png')]:
        case = next(c for c in cases if c['style'] == style and c['target'] == target)
        old_root = ROOT / 'output/aura-reference-grades-retry-2026-10-22'
        images = [(matrix_root / f'{style}-reference.png', 'Uploaded reference'),
            (matrix_root / f'{style}-{target}-expected.png', 'Known grade on this target'),
            (old_root / f'{style}-{target}-matched.png', 'Old native match'),
            (matrix_root / f'{style}-{target}-matched.png', 'Revised native match')]
        cards.append('<section><h2>Controlled native renders: ' + style + ' / ' + target + '</h2><p>Mean known-grade CIE76: ' + f'{case["oldMatcherMeanCie76"]:.3f} → {case["newMatcherMeanCie76"]:.3f}' + '. These PNGs record native renders; the Internet and RAW panels below show actual app exports.</p><div class="row">' + ''.join('<figure><a href="' + Path(os.path.relpath(path, OUT)).as_posix() + '"><img loading="lazy" src="' + Path(os.path.relpath(path, OUT)).as_posix() + '"></a><figcaption>' + caption + '</figcaption></figure>' for path,caption in images) + '</div></section>')
    for e in exports:
        name = Path(e['reference']).stem
        reference = next(r for r in gui['downloadedReferences'] if r['name'] == name)
        previous = ROOT / 'output/aura-internet-reference-2026-10-22/ui-qualified' / ('export-' + name)
        manifest = read(previous / 'aura-delivery-manifest.json')
        index = next(i for i,p in enumerate(gui['photos']) if p['fileName'] == e['target'])
        images = [(reference['file'], 'Reference'), (gui_root / (e['target'] + '-before.png'), 'Target before matching'),
                  (previous / manifest['files'][index]['path'], 'Old matcher export'), (e['file'], 'Revised matcher export')]
        cards.append('<section><h2>' + html.escape(name + ' / ' + e['target']) + '</h2><p>Reference: <a href="' + reference['page'] + '">' + html.escape(reference['photographer']) + '</a> · <a href="' + reference['licenseUrl'] + '">licence</a></p><div class="row">' + ''.join('<figure><a href="' + Path(os.path.relpath(path,OUT)).as_posix() + '"><img loading="lazy" src="' + Path(os.path.relpath(path,OUT)).as_posix() + '"></a><figcaption>' + caption + '</figcaption></figure>' for path,caption in images) + '</div></section>')
    cards.append('<section><h2>Original RAW export</h2><p><a href="989A2865-adaptive-full.jpg">Full-resolution matched JPEG</a></p><div class="raw row"><figure><img src="raw-before-preview.png"><figcaption>Before reference matching</figcaption></figure><figure><img src="raw-exported-jpeg-preview.png"><figcaption>Actual delivered JPEG (display preview)</figcaption></figure></div></section>')
    (OUT / 'index.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AURA adaptive colour matching</title><style>body{font:16px/1.5 system-ui;background:#19171d;color:#eee;max-width:1500px;margin:28px auto;padding:0 20px}a{color:#cbb5ff}.row{display:flex;gap:12px}figure{width:25%;margin:0}img{width:100%}figcaption{color:#c5bdce}.raw figure{width:50%}section{border-top:1px solid #595060;margin-top:32px}strong{color:#ffd091}@media(max-width:800px){.row{flex-wrap:wrap}figure{width:calc(50% - 12px)}}</style><h1>AURA adaptive colour matching</h1><p><strong>Universal exact matching remains unverified.</strong> These are actual exported results. The revised code fits signed parameters, separate curves/bands and tone-dependent casts with content regularization and native checks.</p><p><a href="../../docs/reports/AURA-adaptive-colour-matching-2026-10-22.md">Measured report</a> · <a href="../aura-adaptive-reference-final-2026-10-22/index.html">18 known-grade comparisons</a></p>''' + ''.join(cards) + '</html>', encoding='utf-8')
    print(json.dumps(totals))


if __name__ == '__main__':
    main()
