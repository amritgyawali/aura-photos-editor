"""Publish the colour-match evidence and an offline before/after gallery."""
import argparse
import csv
import hashlib
import html
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'output/aura-internet-reference-2026-10-22'
CHECKS = ROOT / '.work-checks/aura-internet-reference-2026-10-22'
REPORT = ROOT / 'docs/reports/AURA-reference-colour-match-2026-10-22'


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', default='ui-qualified')
    args = parser.parse_args()
    run = BASE / args.run
    gui = read(run / 'results.json')
    grades_path = ROOT / 'output/aura-reference-grades-retry-2026-10-22'
    grades = read(grades_path / 'results.json')
    assert gui['completed'] and gui['originalsUnchanged'] and not gui['pageErrors']
    assert grades['completed'] and grades['originalsUnchanged']
    native = next(json.loads(line) for line in (CHECKS / 'native-raw-reference.log').read_text(encoding='utf-8').splitlines()
                  if line.startswith('{') and 'fullResolutionChecked' in line)
    native['reference'].pop('thumbnail', None)
    raw_inputs = read(CHECKS / 'native-raw-inputs.json')
    assert native['fullResolutionChecked'] and raw_inputs['exitCode'] == 0 and raw_inputs['originalUnchanged']
    cases = []
    for row in grades['cases']:
        before, after = row['baselineToKnownGrade'], row['matchedToKnownGrade']
        cases.append(dict(style=row['style'], target=row['target'], sameImage=row['sameImage'],
                          beforeMeanCie76=before['meanDeltaE76'], afterMeanCie76=after['meanDeltaE76'],
                          closerToKnownGrade=after['meanDeltaE76'] < before['meanDeltaE76'],
                          exactPixels=after['exactPixels'], amount=row['acceptedAmount'],
                          functionalPass=row['passed'], zeroRestoresExact=row['zeroRestoresExact']))
    new_targets = [c for c in cases if not c['sameImage']]
    controls = gui.get('controls', [])
    totals = dict(guiSteps=len(gui['steps']), guiPassed=sum(s['passed'] for s in gui['steps']),
                  internetTransfers=len(gui['matches']), verifiedJpegs=4,
                  independentControlChecks=len(controls), controlsAccepted=sum(c['ok'] for c in controls),
                  controlsExplicitlyRefused=sum(not c['ok'] for c in controls), controlledTransfers=len(cases),
                  closerToKnownGrade=sum(c['closerToKnownGrade'] for c in cases),
                  differentPhotoTransfers=len(new_targets), differentPhotoCloser=sum(c['closerToKnownGrade'] for c in new_targets),
                  pixelExact=sum(c['exactPixels'] for c in cases))
    checks = {name: read(CHECKS / (name + '.json')) for name in ['ui-tests-retry', 'ui-build', 'desktop-build', 'native-raw-reference', 'native-reference-integration', 'desktop-' + args.run]}
    assert all(c['exitCode'] == 0 for c in checks.values())
    sources = ['ui/src/App.tsx', 'ui/src/components/develop/PhotoStudio.tsx', 'ui/src/components/develop/PhotoStudio.test.tsx',
               'scripts/verify-aura-internet-color-match.py', 'scripts/select-native-reference.py',
               'scripts/audit-aura-reference-grades.py', 'scripts/report-aura-reference-match.py']
    evidence = dict(date='2026-10-22', conclusion='Approximate appearance transfer works; arbitrary exact grade cloning and Evoto parity are not established.',
                    totals=totals, internetRun=str(run.relative_to(ROOT)), gui=gui, controlledCases=cases,
                    controlledMetric=grades['metric'], formats=grades['formats'], rawTest=native, rawInputs=raw_inputs,
                    checks=checks, controlledExecutableSha256=grades['executableSha256'],
                    sourceSha256={path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in sources})
    REPORT.with_suffix('.json').write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    with Path(str(REPORT) + '-cases.csv').open('w', encoding='utf-8', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(cases[0]))
        writer.writeheader()
        writer.writerows(cases)
    refs = gui['downloadedReferences']
    ref_lines = '\n'.join(f"| [{r['name']}]({r['page']}) | {r['photographer']} | {r['width']} × {r['height']} | {r['bytes']:,} |" for r in refs)
    transfer_lines = '\n'.join(f"| {r['reference']} | {r['target']} | {r['match']['beforeDistance']:.3f} → {r['match']['afterDistance']:.3f} | {r['acceptedControls']['amount']:.2f} |" for r in gui['matches'])
    grade_lines = '\n'.join(f"| {c['style']} | {c['target']} | {'same source' if c['sameImage'] else 'new photo'} | {c['beforeMeanCie76']:.3f} | {c['afterMeanCie76']:.3f} | {'closer' if c['closerToKnownGrade'] else 'farther'} |" for c in cases)
    step_lines = '\n'.join(f"- PASS: {s['name']}." for s in gui['steps'])
    report = f'''# AURA reference colour matching — 2026-10-22

AURA successfully imported two original-resolution Internet references, applied each to two supplied-photo previews, saved and exported the edits, and matched one native CR3 without modifying its source. The main **Match a look** page now exposes single-image matching beside the shared editor preview.

**Quality verdict: approximate appearance matching, not exact cloning of any grade.** In the controlled known-grade experiment, {totals['closerToKnownGrade']}/{len(cases)} matches moved closer to the known edit; {totals['differentPhotoCloser']}/{len(new_targets)} different-photo transfers improved and 3 became farther from that edit. None of the 18 was pixel exact. Strong split-toning and monochrome reconstruction remain limitations. Exact Evoto parity and perfection have not been demonstrated.

## Repair and scope

- `App.tsx`: bring the existing single-image reference controls and filmstrip into **Match a look**; keep folder/Instagram style learning available in a separate disclosure.
- `PhotoStudio.tsx`: reuse the mounted editor, preview and history; show reference tools in this workspace, close active drawing/picker modes on entry and preserve the previous editing panel for return.
- A regression verifies that switching workspace does not create another preview renderer or write an edit, and restores the previous editing panel.
- The native matching algorithm and its skin/highlight guards were retained. The Windows picker automation was corrected to send edit notifications and Enter, then verify the actual selected path; this was a test-helper fix, not an established app-picker defect.

## High-quality downloaded references

| Reference/source | Photographer | Original dimensions | Download bytes |
|---|---|---:|---:|
{ref_lines}

Downloaded from the original Pexels image URLs, verified as JPEGs with original dimensions and no upscaling. Credits, URLs, [Pexels licence](https://www.pexels.com/license/) and SHA-256 values are retained in [the machine report](AURA-reference-colour-match-2026-10-22.json) and `output/aura-internet-reference-2026-10-22/download-manifest.json`. These are finished reference looks; their original adjustment recipes are unknown. The warm reference includes strong orange scene lighting.

## Actual desktop workflow

{totals['guiPassed']}/{totals['guiSteps']} recorded GUI steps passed, with no JavaScript page errors. The GUI targets are 1024-pixel proxies made from the supplied `989A2698.CR3` and `989A2865.CR3` before previews; this is distinct from the native RAW check below. Import preserved each reference's original dimensions; analysis internally samples the reference. Four verified JPEGs were exported through the app's Export page, with delivery manifests and an independent image decode. Analysis does not change history; matching retains geometry, retouch and masks; Undo/Redo restores exact recipes and amount zero restores exact baseline pixels. Reload retains recipes and strengths.

{step_lines}

{len(controls)} independent control checks covered all four main strengths at 0%/200% and all twelve fine strengths at 0%, with the other strengths reset to defaults. {totals['controlsAccepted']} saved and {totals['controlsExplicitlyRefused']} were explicitly refused by quality guards; each refusal left recipe and history unchanged. Acceptance proves persistence and validation, not that every fitted component has a nonzero visual effect.

| Reference | Target | Internal appearance distance | Accepted amount |
|---|---|---:|---:|
{transfer_lines}

Internal distance measures the app's regional appearance descriptor; it is **not a pixel error, quality percentage or proof of equal colour**. Different scenes have different subjects, materials and lighting. No pixel-similarity score is assigned to these unrelated Internet and target images.

Offline gallery: [`output/aura-internet-reference-2026-10-22/index.html`](../../output/aura-internet-reference-2026-10-22/index.html). Original downloads, before/after PNGs, verified JPEG deliveries, recipes, screenshots and a saved two-photo review catalogue remain there. Primary run: `{run.relative_to(ROOT).as_posix()}`. `catalog/qualified.sqlite` was backed up before control and manual-edit stress checks, retaining the four-match workflow's final warm look for review.

## Native RAW check

Explicitly ran the opt-in test `native_raw_reference_match_does_not_touch_originals` with `H:\\AURA EDIT\\989A2865.CR3` and the downloaded 4000 × 6000 warm reference: **1 passed, 0 failed**. Amount {native['controls']['amount']:.2f}; native-resolution validation true; internal descriptor {native['beforeDistance']:.6f} → {native['afterDistance']:.6f}. Source SHA-256 before and after: `{raw_inputs['rawBeforeSha256']}`. The reference hash also matches the download manifest. This validates this RAW/reference pair, not every camera format or every possible look.

## Controlled known-grade quality experiment

Six edits were generated with AURA's own renderer: warm WB/tint, cool WB/contrast, matte curve/desaturation, teal/orange split tone, punchy contrast/saturation, monochrome. Each reference was uploaded and matched to its source and two different supplied-photo proxies. For each target the oracle is the same known adjustment applied directly to that target. Aligned mean CIE76 is a reproducible difference measure, not a perfection score. An adaptive matcher may choose a different edit; this experiment tests the specific request to clone a known treatment. Warm/cool fixtures intentionally exceed conservative fitting limits.

| Grade | Target | Relationship | Before mean CIE76 | Matched mean CIE76 | Result |
|---|---|---|---:|---:|---|
{grade_lines}

All 18 completed functional checks at default amount 1.0, including read-only analysis, native validation, exact Undo/Redo and exact zero removal. Quality and functionality are reported separately. The split-tone cases on both new photos and the punchy case on target 1 were farther from the known edit; monochrome remained strongly different despite some error reduction. See [all cases as CSV](AURA-reference-colour-match-2026-10-22-cases.csv) and the [controlled gallery](../../output/aura-reference-grades-retry-2026-10-22/index.html).

## Input support and limits

JPEG, opaque PNG and TIFF imports were exercised. An embedded sRGB ICC profile was converted successfully. WebP, translucent PNG and corrupt JPEG were refused with explicit errors and no reference added. This is **not support for any image format**. The decoder also specifies bounds (256 MB, 64 megapixels, 16000 pixels per side) and ICC/colour-space checks; those bounds were inspected, not all exercised in this run.

The fitter estimates bounded tone, HSL and warmth/tint adjustments. Its conservative style delta limits include approximately ±800 K temperature, ±0.67 EV exposure and bounded colour/tone adjustments. It does not recover an arbitrary original 3D LUT, a complete editing recipe or exact original camera white balance from a finished image. It cannot reconstruct scene lighting from another photograph. Skin/highlight safety may refuse a strength combination. Matching remains useful for a starting look, with visual review and manual finishing required for stronger grades.

## Validation and provenance

- Frontend: **1052 tests passed in 111 files**, including the new workspace/preview regression.
- Frontend production build and native desktop build passed. Existing linker warning about multiple non-default manifests remains.
- Current native reference integration suite: **5 passed, 0 failed, 1 opt-in RAW test skipped** in that invocation. The RAW test was explicitly run separately above and passed. These tests cover regional strengths/masks, capture/library metadata/reopen, different dimensions/manual edits, framing and finishing history.
- Current tested desktop SHA-256: `{gui['executableSha256']}`.
- The controlled matrix ran before the UI entrypoint repair on desktop `{grades['executableSha256']}`; native fitting/backend code did not change between the matrix and current build.
- Logs and command metadata: `.work-checks/aura-internet-reference-2026-10-22/`; controlled matrix: `.work-checks/aura-reference-grades-2026-10-22/`.
- Retained excluded attempts: initial matrix assumed independent strengths must always pass guards; Windows picker helper initially selected the previous file; immutable Tauri-invoke instrumentation did not observe calls; expanded helper queried collapsed fine controls using visible-only locators and omitted reselecting the collection after UI reload. These attempts were corrected and rerun, and are not counted as product passes or product defects. Further excluded attempts, if present, remain in the same logs.
- The earlier [comprehensive audit](AURA-comprehensive-test-2026-10-22.md) retains its historical test totals. This supplemental run does not relabel that audit as a current full native-suite rerun.
'''
    REPORT.with_suffix('.md').write_text(report, encoding='utf-8')
    cards = []
    for reference in refs:
        name = Path(reference['file']).name
        style = Path(name).stem
        cards.append(f'<section><h2>{html.escape(style)}</h2><p>Reference by <a href="{reference["page"]}">{html.escape(reference["photographer"])}</a> · {reference["width"]} × {reference["height"]} · <a href="{name}">original download</a> · <a href="{reference["licenseUrl"]}">licence</a></p><img class="reference" src="{name}" alt="Downloaded colour reference">')
        delivery = read(run / ('export-' + style) / 'aura-delivery-manifest.json')
        assert delivery['verified'] and delivery['file_count'] == 2
        for index in range(2):
            row = next(m for m in gui['matches'] if m['reference'] == name and m['target'] == f'target-{index}.jpg')
            before = f'{args.run}/target-{index}.jpg-before.png'
            matched = f'{args.run}/{style}-target-{index}.jpg-matched.png'
            export = f'{args.run}/export-{style}/' + delivery['files'][index]['path']
            cards.append(f'<article><h3>Target {index + 1}: {"989A2698" if index == 0 else "989A2865"}</h3><div class="pair"><figure><img src="{before}" alt="Before reference matching"><figcaption>Before matching (initial corrections saved)</figcaption></figure><figure><img src="{matched}" alt="AURA reference match"><figcaption>AURA matched appearance</figcaption></figure></div><p>Internal descriptor: {row["match"]["beforeDistance"]:.3f} → {row["match"]["afterDistance"]:.3f}; amount {row["acceptedControls"]["amount"] * 100:.0f}%. <a href="{export}">Verified JPEG</a> · <a href="{args.run}/{style}-target-{index}.jpg.recipe.json">Saved recipe</a></p></article>')
        cards.append('</section>')
    (BASE / 'index.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AURA Internet colour reference tests</title><style>body{font:16px/1.5 system-ui;background:#18171d;color:#ece8f3;max-width:1200px;margin:32px auto;padding:0 20px}a{color:#d1b6ff}.reference{max-height:600px;max-width:100%;display:block}.pair{display:flex;gap:20px}.pair figure{width:50%;margin:0}.pair img{width:100%}figcaption{color:#bcb7c7}section{border-top:1px solid #595366;margin-top:40px}article{margin:30px 0}h1,h2,h3{line-height:1.2}strong{color:#f3c983}@media(max-width:600px){.pair{display:block}.pair figure{width:100%}}</style><h1>AURA colour reference matching</h1><p>Two original-resolution Internet references, each applied through AURA to two supplied-photo previews. Originals were preserved; four JPEGs passed export verification.</p><p><strong>Approximate transfer, not an exact grade clone.</strong> Lighting, subjects and colour distributions differ. The internal descriptor is not a visual-quality percentage. Strong split tones and monochrome reconstruction remain limitations.</p><p><a href="../../docs/reports/AURA-reference-colour-match-2026-10-22.md">Full report</a> · <a href="../aura-reference-grades-retry-2026-10-22/index.html">18 controlled known-grade comparisons</a> · <a href="download-manifest.json">Download provenance</a></p>''' + ''.join(cards) + '</html>', encoding='utf-8')
    print(json.dumps(totals))


if __name__ == '__main__':
    main()
