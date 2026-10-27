"""Write the tracked report and all-pair CSV from completed native assessments."""
import argparse
import csv
import html
import importlib.util
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
STYLES = ['warm-matte', 'cool-cyan', 'teal-orange', 'pastel', 'warm-punch', 'monochrome']


def load(path):
    raw = json.loads((path / 'results.json').read_text(encoding='utf-8'))
    measured = json.loads((path / 'independent-analysis.json').read_text(encoding='utf-8'))
    assert not raw.get('failure') and all(raw['checks'].values()) and len(raw['pairs']) == 120
    return raw, measured


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('before', 'after', 'paired'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--gui', type=Path, help='Completed GUI assessment directory, including explicit refused switches.')
    parser.add_argument('--gui-failure', type=Path, action='append', default=[], help='Preserve a preliminary failed GUI attempt in the report.')
    args = parser.parse_args()
    runs = {name: load(getattr(args, name).resolve()) for name in ('before', 'after', 'paired')}
    for index in range(1, 21):
        with Image.open(args.before / f'{index:02}.before.png') as old, Image.open(args.after / f'{index:02}.before.png') as new:
            assert old.size == new.size and old.tobytes() == new.tobytes(), 'Before/after assessment baselines differ'
    regressions = []
    for row in runs['after'][0]['pairs']:
        t = row.get('native', {}).get('fitting', {}).get('transfer', {})
        if t.get('renderedAfter', {}).get('palette', 0) > t.get('renderedBefore', {}).get('palette', 0) + .0001:
            regressions.append((row['targetIndex'], row['style']))
    assert not regressions, 'A corrected native palette endpoint regressed'
    changed_outputs = []
    for row in runs['after'][0]['pairs']:
        name = f'{row["stem"]}.matched.png'
        with Image.open(args.before / name) as old, Image.open(args.after / name) as new:
            if old.size != new.size or old.tobytes() != new.tobytes():
                changed_outputs.append((row['targetIndex'], row['style']))
    report_path = ROOT / 'docs/reports/AURA-twenty-image-colour-assessment-2026-10-24.md'
    csv_path = report_path.with_name('AURA-twenty-image-colour-pairs-2026-10-24.csv')
    fields = ['workflow', 'target', 'photo_id', 'reference_photo_id', 'grade', 'exact_known_edit', 'close_known_edit', 'reference_palette_and_tone_close', 'verdict',
              'reference_color_before', 'reference_color_after', 'reference_tone_before', 'reference_tone_after', 'meaningful_color_signal', 'joint_color_before', 'joint_color_after',
              'oracle_mean_delta_e76', 'oracle_p95_delta_e76', 'oracle_tone_error', 'oracle_chroma_error',
              'known_edit_improvement_percent', 'native_palette_before', 'native_palette_after', 'native_total_before', 'native_total_after',
              'native_backtrack_factor', 'calibrated_amount', 'diagnosis']
    with csv_path.open('w', encoding='utf-8', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for workflow, (raw, analysis) in runs.items():
            native = {(x['targetIndex'], x['style']): x for x in raw['pairs']}
            for row in sorted(analysis['pairs'], key=lambda x: (x['targetIndex'], x['style'])):
                original = native[(row['targetIndex'], row['style'])]
                transfer = original.get('native', {}).get('fitting', {}).get('transfer', {})
                b, a = transfer.get('renderedBefore', {}), transfer.get('renderedAfter', {})
                writer.writerow(dict(workflow=workflow, target=row['targetIndex'], photo_id=row['photoId'],
                                     reference_photo_id=original['referencePhotoId'], grade=row['style'],
                                     exact_known_edit=row['oracle']['bitwiseIdentical'], close_known_edit=row['oracle']['closeKnownLook'],
                                     reference_palette_and_tone_close=row['closeReferencePaletteAndTone'],
                                     verdict=row['verdict'], reference_color_before=row['paletteBefore']['color'], reference_color_after=row['paletteAfter']['color'],
                                     reference_tone_before=row['paletteBefore']['tone'], reference_tone_after=row['paletteAfter']['tone'],
                                     meaningful_color_signal=row['meaningfulColorSignal'], joint_color_before=row['jointColorDistanceBefore'], joint_color_after=row['jointColorDistanceAfter'],
                                     oracle_mean_delta_e76=row['oracle']['meanDeltaE76'], oracle_p95_delta_e76=row['oracle']['p95DeltaE76'],
                                     oracle_tone_error=row['oracle']['meanAbsToneError'], oracle_chroma_error=row['oracle']['meanChromaPlaneError'],
                                     known_edit_improvement_percent=row['oracleImprovementPercent'], native_palette_before=b.get('palette'), native_palette_after=a.get('palette'),
                                     native_total_before=b.get('total'), native_total_after=a.get('total'), native_backtrack_factor=row.get('backtrackFactor'),
                                     calibrated_amount=row.get('calibratedAmount'), diagnosis='; '.join(row['reasons'])))
    before, after, paired = (runs[name][1]['summary'] for name in ('before', 'after', 'paired'))
    lines = ['# Twenty new photographs: reference colour matching assessment', '',
             'Date: 2026-10-24. **Reference and target colours do not match perfectly in every case.** This assessment tests 20 additional real photographs separately against six edited, different reference photographs. It records 120 original palette tests, 120 palette retests after an acceptance fix, and 120 tests of the existing original + edited reference calibration workflow. No target is used as its own reference or training source.', '',
             '## Result', '',
             '| Workflow | Exact known edit | Close known edit | Close reference palette + tone | Meaningful reference colour comparisons improved | Known edit closer than baseline |',
             '|---|---:|---:|---:|---:|---:|']
    for label, summary in [('Palette before fix', before), ('Palette after fix', after), ('Paired calibration, separate pipeline', paired)]:
        lines.append(f'| {label} | {summary["exact"]}/120 | {summary["close"]}/120 | {summary["closeReferencePaletteAndTone"]}/120 | {summary["meaningfulColorImproved"]}/{summary["meaningfulColorComparisons"]} | {summary["knownEditImproved"]}/120 |')
    lines += ['', '**Exact** means identical RGB8 pixels to the expected edit on that target. **Close known edit** requires mean CIE76 error ≤3 and p95 ≤8, both measured on all target pixels. **Close reference palette + tone** requires colour-quantile distance ≤3, tone-quantile distance ≤3, and joint a/b distance ≤2. These are declared QA cutoffs, not universal perceptual thresholds or proof that every object has the right colour. The CSV answers both close-match questions separately for every pair. Reference-colour improvement measures resemblance to the separate reference; known-edit recovery measures the intended operation on the target. They answer different questions. Neither is an accuracy percentage.', '',
              'The calibration workflow has an automatic correction stage. Its reference edits and target oracles were generated from that workflow’s native zero-strength baselines. The palette workflow starts from its original native baseline. These are tests of two complete pipelines with their own controlled expectations, **not an isolated, identical-input algorithm A/B**. Calibration results used the pre-fix executable; the changed palette acceptance helper is not called by that workflow.', '',
              'A shared editing operation does not give unrelated scenes identical colour proportions. Therefore, calibration can closely reproduce the known edit while failing the direct reference-palette test. Conversely, palette resemblance can improve while the known edit is missed.', '',
              '## Photos, references and method', '',
              'The new sources include night portraits and city lights, a cafe, a beach at dusk, flowers in snow, two different waterfalls, desert dunes, elderly portraits, an already monochrome portrait, bright and dark food photographs, and studio portraits under varied lighting. All 26 photo IDs differ from the previous 11-photo assessment. Byte/pixel hashes and 611 pairwise perceptual screens cover the 26 new photos and comparisons against the previous set; no duplicate flags were found. The contact sheet was visually reviewed. This is duplicate screening, not a universal image-identity detector.', '',
              'Official Pexels CDN source downloads were preserved at up to 1280 pixels. Inputs preserve framing and aspect, are oriented sRGB JPEG95 without chroma subsampling, and have a long edge of at most 960 pixels. “Full/native resolution” below means the complete imported assessment image, not a camera RAW or full original Pexels master. Published sources may already contain edits; the controlled oracle tests the **additional declared grade**.', '',
              'Each reference has its own source photograph. Six deterministic encoded-sRGB edits are implemented explicitly in `scripts/test-color-match-twenty.py`: warm matte, cool cyan, luminance-dependent teal/orange split tone, desaturated lifted pastel, warm high contrast/saturation, and monochrome. They change colour/tone only. The unpaired matcher receives only the edited reference pixels, never the edit parameters, unedited reference, or target oracle. The calibration workflow receives an aligned original/edited reference pair, with both photographs separate from every target.', '',
              'Each target is reset before every pair. Complete editing parameters and protection fields are checked; provenance source alone may change because Reset is a user action. Full original render pixels must match exactly. Each pair runs native fitting/application, full export-purpose rendering, independent pixel measurements and native full-preview parity. All 20 saved targets are then exported through native Delivery as verified lossless sRGB PNGs, once per reference.', '',
              'Independent measurements use CIE Lab with a D65 white point: five Lab quantiles, colour-only a/b quantiles, 32 signed joint a/b projections of 2048 deterministic stratified samples, full-image target/oracle CIE76 mean and p95, and separate shadow/midtone/highlight residuals. A 0.1-unit colour-error floor marks near-neutral comparisons where a percentage would magnify numerical noise. In particular, the monochrome target already had neutral chroma; an apparent relative chroma improvement is not a meaningful colour achievement.', '',
              '## Concrete bug found and fixed', '',
              'Target 17 (Pexels 32972578, dim studio) with the teal/orange fruit reference exposed an acceptance error. The native combined distance improved **11.517659 → 7.942121**, while native colour-palette distance worsened **5.125721 → 5.451486**. Independent colour quantile error also worsened **33.0114 → 36.6042**; joint a/b distance worsened **20.4799 → 21.2036**. A large tone improvement concealed a colour regression.', '',
              'Native acceptance and the in-process proof cache now require **both total and palette distances to be non-regressing**, using the existing 0.0001 numerical tolerance. Natural mode has a zero palette term, preserving its previous policy. Fitting can backtrack or reject a proposal; a rejected proposal cannot replace the saved edit. Skin/highlight thresholds and distance weights were not changed. Palette diagnostics are versioned `reference-palette-transport-v5`; live matching and Sync refit saved v3/v4 palette sessions. A regression test uses the measured failing distances and verifies that the cache cannot certify them.', '']
    fixed_bad = next(x for x in runs['after'][1]['pairs'] if x['targetIndex'] == 17 and x['style'] == 'teal-orange')
    lines += [f'The retest of that pair produced colour quantile error **{fixed_bad["paletteBefore"]["color"]:.4f} → {fixed_bad["paletteAfter"]["color"]:.4f}**, and joint colour distance **{fixed_bad["jointColorDistanceBefore"]:.4f} → {fixed_bad["jointColorDistanceAfter"]:.4f}**. Its known-edit error remains **{fixed_bad["oracle"]["meanDeltaE76"]:.2f} mean / {fixed_bad["oracle"]["p95DeltaE76"]:.2f} p95 CIE76**. This corrects acceptance policy; it does not establish perfect grade recovery. [Actual previous/retested/expected pixels](../../output/aura-twenty-color-match-2026-10-24/regression-fix-comparison.jpg) also show residual contouring in the dark backdrop. The colour guard does not solve that spatial quality problem.', '',
              '## Results by reference grade', '',
              '| Grade | Palette median joint-colour improvement after fix | Palette median known-edit CIE76 | Calibrated close known edit | Calibrated median known-edit CIE76 |',
              '|---|---:|---:|---:|---:|']
    for style in STYLES:
        a = next(x for x in after['styles'] if x['style'] == style)
        p = next(x for x in paired['styles'] if x['style'] == style)
        lines.append(f'| {style} | {a["medianJointColorImprovementPercent"]:+.1f}% | {a["medianOracleDeltaE76"]:.2f} | {p["close"]}/20 | {p["medianOracleDeltaE76"]:.2f} |')
    lines += ['', 'Calibration reference validation is separate from these new target photographs. Its native error uses **OKLab ×100**, not the CIE76 units above. Low error on held-out tiles of one reference does not prove that unseen scenes are covered.', '',
              '| Training reference | Occupied coarse colour cells /64 | Native reference validation mean / p95 |',
              '|---|---:|---:|']
    for reference in runs['paired'][0]['references']:
        c = reference['auraReference']['calibration']
        lines.append(f'| {reference["style"]} | {c["occupiedColorCells"]} | {c["meanError"]:.3f} / {c["p95Error"]:.3f} |')
    lines += ['', 'Reference source credits (each is separate from all 20 targets):', '']
    for reference in runs['after'][0]['references']:
        source = reference['source']
        lines.append(f'- {reference["style"]}: [{source["description"]}, Pexels {source["photoId"]}]({source["sourcePage"]}) — {source["author"]}.')
    manifest = json.loads((args.after.resolve().parent / 'dataset/manifest.json').read_text(encoding='utf-8'))
    for workflow, title in [('after', 'Every target separately: palette retest'), ('paired', 'Every target separately: calibrated grade')]:
        lines += ['', '## ' + title, '', 'Cells are mean target/oracle CIE76, lower is closer. A low mean alone does not pass the close criterion; p95 must also pass. See the CSV for both values, each reference/target ID, direct reference-palette distances, exact/close flags and per-pair diagnosis.', '',
                  '| Target / photo ID | Warm matte | Cool cyan | Teal/orange | Pastel | Warm punch | Mono |',
                  '|---|---:|---:|---:|---:|---:|---:|']
        index = {(x['targetIndex'], x['style']): x for x in runs[workflow][1]['pairs']}
        for item in [x for x in manifest['images'] if x['role'] == 'target']:
            values = ' | '.join(f'{index[(item["index"], style)]["oracle"]["meanDeltaE76"]:.2f}' for style in STYLES)
            lines.append(f'| {item["index"]:02}: {item["description"]} ({item["photoId"]}) | {values} |')
    lines += ['', '## What does not match and how to improve it', '',
              '| Finding | Evidence and consequence | Fix / next improvement |',
              '|---|---|---|',
              '| Scene colours are confused with the edit | Flowers, fruit, walls and water change the reference histogram; forcing that histogram onto another scene can improve palette distance while moving away from the intended grade. | Use the existing original + edited reference workflow when both are available. For unpaired matching, compare compatible measured regions and retain uncertainty instead of labelling histogram similarity exact. |',
              '| Different scene brightness changes the inferred tone curve | Dark/night subjects can be lifted toward a bright reference; monochrome outputs can be fully desaturated but still miss brightness, black point and contrast. | Separate scene exposure normalization from style tone; fit tone using corresponding neutral/subject regions or paired edit evidence. Keep the Natural option available when scene brightness should remain. |',
              '| Split tones and hue changes remain imperfect | The oracle tests shadows, midtones and highlights separately. The unpaired chromatic lattice is a 2D a/b mapping, so its displacement does not independently model every lightness-dependent colour operation. | Extend unpaired residual fitting to a bounded lightness-conditioned/3D mapping, with held-out colour and gradient checks. The existing paired calibrated model already provides a different route for global RGB mappings. |',
              '| Protection attenuates requested grades | Native backtrack factors and calibrated saved amounts are recorded per pair. Some desired casts/contrast edits cannot be retained at full strength under the current skin/clipping limits. | Improve semantic subject masks and separate creative grading from unintended damage in the acceptance objective. Do not silently remove current checks or claim a limited endpoint is a full-strength clone. |',
              '| Small subjects lack measured face/body regions | Targets 17 and 20 visibly contain people, but their native matched-region lists contain only whole image/background in these tests. Colour-based protection is not equivalent to reliable subject segmentation. | Improve multiscale face/person detection and mask confidence at native resolution; allow explicit corresponding-region anchors when automatic evidence is missing. Recheck skin colour on small subjects separately. |',
              '| Dark-backdrop contouring remains | Visual review of target 17 shows colour bands/patches after a strong unrelated-scene palette transfer, including the corrected output. Aggregate colour metrics do not capture this defect. | Add native flat-region/gradient quality checks; isolate amplification of source JPEG quantization, tone/chroma slopes, gamut compression and final quantization before changing the renderer. Existing gamut mapping already compresses chroma; the precise source of this example’s contouring is not established here. |',
              '| One training photo has incomplete colour coverage | Calibration can fit its reference well but extrapolate poorly to colours/luminances absent from that reference. Native calibration reports occupied colour cells and held-out error. | Learn from multiple aligned original/edited examples with varied illumination and colour coverage; evaluate on distinct held-out photographs and report out-of-coverage cases. This remains additional work. |',
              '| Unknown physical white balance and clipped/local edits cannot be uniquely recovered | These are rendered JPEG/PNG images without camera/lighting ground truth; multiple edit recipes can yield the same reference. | Offer explicit neutral/skin region anchors and colour-managed camera baselines. Test RAW, ICC/wide-gamut, HDR and local edits separately before extending claims. |', '',
              'The implemented change fixes the observed colour-regression acceptance bug. The architectural improvements above are recommendations, not claims of completed algorithm changes. An edited image of an unrelated scene alone does not uniquely identify its original colours, illumination and editing operation. Consequently, a universal 100% exact-transfer guarantee would be unsupported.', '',
              'For the current app: use **Use original + edited pair** when both versions of the reference are available, keeping all targets separate from that training photograph. Use **Reference palette and tone** when intentionally transferring scene colour proportions; choose **Natural look** when preserving the target scene’s tonal balance matters. Inspect the result and use the independent tone, colour and warmth strengths to address residuals. A red/green or blue/yellow residual in the CSV describes measured colour bias; it does not uniquely identify a physical white-balance error.', '',
              '## Verification and reproducibility', '']
    lines += [f'All 20 original baseline renders were byte-identical across the before/after assessments. Independent retesting changed {len(changed_outputs)} of 120 output images; {120-len(changed_outputs)} outputs were byte-identical to the previous engine. Changed pairs: {changed_outputs}. The final retest accepted **zero native colour-palette regressions** under the declared tolerance.', '',
              'Code validation: four native proof/cache tests passed, including the measured regression; native colour-match integration tests passed (10 passed, one optional RAW test skipped because its external fixtures were not provided); the focused UI suite passed 39 tests, including v4 live-session and Sync migrations. TypeScript/Vite production build and desktop build succeeded. Existing frontend chunk-size and GNU linker resource-manifest warnings remain. Python assessment scripts compiled successfully.', '']
    if args.gui:
        gui = json.loads((args.gui / 'results.json').read_text(encoding='utf-8'))
        assert not gui.get('failure') and all(gui['checks'].values())
        relative = args.gui.resolve().relative_to(ROOT).as_posix()
        lines += [f'Real UI follow-up: [results](../../{relative}/results.json) and [workspace](../../{relative}/workspace.png). Quick one-click matching, live 75% strength, saved preset, Precise refitting, collection Sync and original-file checks passed on two new targets with bounded 640-pixel GUI fixtures. Reference-switch outcome: **{gui.get("referenceSwitchOutcome", "not requested")}**. This is separate from the 360 full-imported-image pair assessments.', '']
        if gui.get('referenceSwitchRefusal'):
            lines += [f'The warm-to-cool reference switch was not a successful match: `{gui["referenceSwitchRefusal"]}`. The saved recipe hash and previous session ID were verified unchanged, and the UI continued to label the new reference unapplied. Do not interpret this refusal-preservation check as successful colour transfer.', '']
        response = gui.get('responsiveness', {})
        lines += [f'GUI frame observations: {response.get("visibleFrameSamples")} visible samples, median gap {response.get("medianGapMilliseconds")} ms, p95 {response.get("p95GapMilliseconds")} ms, maximum {response.get("maximumGapMilliseconds")} ms. These measurements include occasional stalls and are not a guarantee of smoothness on other workloads.', '']
    for failure in args.gui_failure:
        raw = json.loads((failure / 'results.json').read_text(encoding='utf-8'))
        relative = failure.resolve().relative_to(ROOT).as_posix()
        lines += [f'Preliminary GUI attempt retained: [{failure.name}](../../{relative}/results.json): `{raw.get("failure")}`; desktop exit before cleanup `{raw.get("desktopExitCodeBeforeCleanup")}`. A window/connection closed unexpectedly in one attempt; its cause was not established. Another attempt stayed open but the old harness timed out waiting for success despite the explicit native refusal. The harness now detects alerts promptly and records refusals separately.', '']
    for workflow, (raw, analysis) in runs.items():
        run = getattr(args, workflow).resolve()
        relative = run.relative_to(ROOT).as_posix()
        lines += [f'- **{workflow}**: [local gallery](../../{relative}/index.html), [raw native results](../../{relative}/results.json), [independent analysis](../../{relative}/independent-analysis.json). Executable SHA256 `{raw["exeSha256"]}`. Checks: all 120 pairs, distinct references/targets, original baseline before each pair, native preview/export pixel parity, 120 verified Delivery PNGs, original source/input hashes unchanged and no JavaScript page errors. Desktop exit before controlled cleanup: `{raw["desktopExitCodeBeforeCleanup"]}`.',
                  f'  Observed median analysis {analysis["summary"]["medianAnalysisMilliseconds"] if analysis["summary"]["medianAnalysisMilliseconds"] is not None else "not applicable (one shared calibration per reference)"} ms; median Apply {analysis["summary"]["medianApplyMilliseconds"]:.1f} ms. Debug-build observations with other local verification work; not a controlled throughput benchmark.']
    lines += ['', 'The original palette run paused after its first reference because the harness incorrectly required Reset provenance hashes to remain identical. It resumed only after verifying original pixel equality and all parameter/protection fields; the earlier failure is retained as `previousRunFailure`. No failed result was silently replaced by a pass.', '',
              '[All 360 pair measurements and diagnoses](AURA-twenty-image-colour-pairs-2026-10-24.csv). [Dataset credits](../../output/aura-twenty-color-match-2026-10-24/dataset/ATTRIBUTION.md), [duplicate evidence](../../output/aura-twenty-color-match-2026-10-24/dataset/manifest.json), [source contact sheet](../../output/aura-twenty-color-match-2026-10-24/dataset/contact-sheet.jpg). Pexels permits downloading and modifying these photos under its [licence](https://www.pexels.com/license/); individual photographers and source pages are recorded. Downloaded photographs, catalogues and generated galleries stay in ignored local output; scripts, this report and numerical CSV are tracked.', '',
              'Reproduce from the repository root (Pillow, numpy and Playwright required):', '', '```powershell',
              'python scripts/prepare-color-match-twenty.py',
              '# Inspect dataset/contact-sheet.jpg; record honest visual review in manifest.json.',
              'python scripts/test-color-match-twenty.py --intent palette',
              'python scripts/test-color-match-twenty.py --paired-calibration',
              'python scripts/analyse-color-match-twenty.py <completed-run-directory>',
              'python scripts/report-color-match-twenty.py --before <before-run> --after <after-run> --paired <paired-run>',
              '```', '',
              'Known pre-existing Natural-mode accuracy failures are recorded in the preceding [palette-fix report](AURA-reference-palette-fix-2026-10-24.md). They are not cleared by this assessment. This assessment does not establish proprietary Evoto algorithm equivalence or correctness for all photographs.']
    report_path.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    spec = importlib.util.spec_from_file_location('assessment', ROOT / 'scripts/test-color-match-twenty.py')
    assessment = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(assessment)
    cards = []
    for workflow, (raw, measured) in runs.items():
        directory = getattr(args, workflow).resolve()
        assessment.write_artifacts(directory, raw, manifest)
        s = measured['summary']
        cards.append(f'<article><h2><a href="{directory.name}/index.html">{html.escape(workflow)}: {html.escape(raw["intent"])}</a></h2><p>{s["close"]}/120 close known grades; {s["exact"]} exact. {s["closeReferencePaletteAndTone"]}/120 close reference palettes and tones.</p></article>')
    home = '''<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AURA colour assessment</title><style>body{max-width:1000px;margin:40px auto;padding:20px;background:#17171c;color:#eee;font:17px system-ui;line-height:1.6}a{color:#d9baff}article{padding:12px 0;border-top:1px solid #555}</style><h1>Twenty new photos, six edited references</h1><p>360 independent pairs across the previous palette engine, corrected palette engine and original + edited calibration workflow. Reference and targets are different photographs. Matching is still imperfect. Open a workflow and filter by target or grade to inspect the actual pixels.</p>'''
    home += ''.join(cards) + '<p><a href="../../docs/reports/AURA-twenty-image-colour-assessment-2026-10-24.md">Full report</a> · <a href="../../docs/reports/AURA-twenty-image-colour-pairs-2026-10-24.csv">Every pair: measurements and diagnosis</a> · <a href="dataset/ATTRIBUTION.md">Photo credits</a> · <a href="dataset/contact-sheet.jpg">Unique source photographs</a></p>'
    (args.after.resolve().parent / 'index.html').write_text(home, encoding='utf-8')
    print(json.dumps({'report': str(report_path), 'csv': str(csv_path), 'pairs': 360}), flush=True)


if __name__ == '__main__':
    main()
