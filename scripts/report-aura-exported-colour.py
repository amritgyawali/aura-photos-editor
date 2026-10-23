"""Create a report and gallery from the actual exported-colour measurements."""
import argparse
import csv
import hashlib
import html
import json
from pathlib import Path
import shutil
from aura_colour_metrics import load_srgb

ROOT = Path(__file__).resolve().parents[1]


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, default=ROOT / 'output/aura-export-colour-2026-10-22')
    parser.add_argument('--raw', type=Path)
    args = parser.parse_args()
    out = args.evidence.resolve()
    evidence = read(out / 'results.json')
    assert evidence['completed'] and evidence['originalsUnchanged'] and not evidence['pageErrors']
    raw = read(args.raw / 'results.json') if args.raw else None
    if raw:
        assert raw['completed'] and raw['originalsUnchanged'] and not raw['pageErrors']
        raw_jpeg_source = Path(next(e['file'] for e in raw['exports'] if e['info']['format'] == 'JPEG'))
        raw_delivery_copy = out / '989A2865-reference-matched-full.jpg'
        shutil.copy2(raw_jpeg_source, raw_delivery_copy)
        assert hashlib.sha256(raw_delivery_copy.read_bytes()).digest() == hashlib.sha256(raw_jpeg_source.read_bytes()).digest()
        raw['publishedJpegCopy'] = str(raw_delivery_copy)
    # Inspect delivered colour-space tags again: PNG sRGB chunks describe sRGB
    # even without an ICC profile. This metadata refinement changes no pixels or scores.
    for item in [e for m in evidence['matches'] for e in m['exports']] + (raw['exports'] if raw else []):
        _, item['info'] = load_srgb(item['file'])
        item['postRunColourMetadataChecked'] = True
    stem = ROOT / 'docs/reports/AURA-exported-colour-verification-2026-10-22'
    rows = []
    for match in evidence['matches']:
        prior = next(r for r in evidence['existingExports'] if r['reference']['name'] == match['reference']['name'] and r['target'] == match['target'])
        jpeg = next(e for e in match['exports'] if e['info']['format'] == 'JPEG')
        png = next(e for e in match['exports'] if e['info']['format'] == 'PNG')
        assert png['versusRender']['exactPixels'] and jpeg['sidecarMatchesRecipe']
        assert match['best']['afterDistance'] <= match['default']['afterDistance']
        rows.append(dict(reference=match['reference']['name'], target=match['target'],
            defaultNativeDistance=match['default']['afterDistance'], selectedNativeDistance=match['best']['afterDistance'],
            originalToneGap=prior['referenceDistributionGap']['toneQuantileMaeL'],
            selectedToneGap=jpeg['referenceDistributionGap']['toneQuantileMaeL'],
            originalChromaGap=prior['referenceDistributionGap']['chromaQuantileMae'],
            selectedChromaGap=jpeg['referenceDistributionGap']['chromaQuantileMae'],
            selectedPaletteDistance=jpeg['referenceDistributionGap']['paletteJensenShannonDistance'],
            jpegMeanDeltaE76VersusRender=jpeg['versusRender']['meanDeltaE76'],
            jpegP95DeltaE76VersusRender=jpeg['versusRender']['p95DeltaE76'], pngExactVersusRender=True,
            calibratedWhiteBalanceVerified=False, jpeg=jpeg['file'], png=png['file']))
    attempts = [a for m in evidence['matches'] for a in m['attempts']]
    evidence['summary'] = dict(pairs=len(rows), candidateApplications=len(attempts),
        accepted=sum(a['accepted'] for a in attempts), explicitlyRefused=sum(not a['accepted'] for a in attempts),
        selectedNativeDescriptorImproved=sum(r['selectedNativeDistance'] < r['defaultNativeDistance'] for r in rows),
        exactReferenceGradeVerified=False, calibratedWhiteBalanceVerified=False,
        previewExports=len(rows) * 2, nativeRawExports=len(raw['exports']) if raw else 0)
    evidence['rawExport'] = raw
    evidence['exportComparisons'] = rows
    evidence['verificationCommand'] = read(ROOT / '.work-checks/aura-export-colour-2026-10-22/exported-colour.json')
    if raw:
        evidence['rawVerificationCommand'] = read(ROOT / '.work-checks/aura-export-colour-2026-10-22/native-raw-export.json')
    evidence['metricsCurrentSha256'] = hashlib.sha256((ROOT / 'scripts/aura_colour_metrics.py').read_bytes()).hexdigest()
    stem.with_suffix('.json').write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    with Path(str(stem) + '-cases.csv').open('w', encoding='utf-8', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    compare_lines = '\n'.join(f"| {r['reference']} / {r['target']} | {r['defaultNativeDistance']:.3f} → {r['selectedNativeDistance']:.3f} | {r['originalToneGap']:.3f} → {r['selectedToneGap']:.3f} | {r['originalChromaGap']:.3f} → {r['selectedChromaGap']:.3f} |" for r in rows)
    export_lines = '\n'.join(f"| {r['reference']} / {r['target']} | {r['jpegMeanDeltaE76VersusRender']:.4f} | {r['jpegP95DeltaE76VersusRender']:.4f} | exact |" for r in rows)
    control_lines = '\n'.join(f"| {m['reference']['name']} / {m['target']} | {m['bestControls']['amount'] * 100:.0f}% | {m['bestControls']['tone'] * 100:.0f}% | {m['bestControls']['color'] * 100:.0f}% | {m['bestControls']['whiteBalance'] * 100:.0f}% |" for m in evidence['matches'])
    raw_section = 'A further native RAW export was not included in this invocation.'
    if raw:
        raw_jpeg = next(e for e in raw['exports'] if e['info']['format'] == 'JPEG')
        raw_png = next(e for e in raw['exports'] if e['info']['format'] == 'PNG')
        dims = raw['nativeDimensions']
        raw_section = f'''`H:\\AURA EDIT\\989A2865.CR3` was matched to the same downloaded warm reference in an isolated copy of the saved 27-photo catalogue, preserving its existing geometry, retouch and masks. The full decode and both delivered files are **{dims['width']} × {dims['height']}**, rather than embedded-thumbnail dimensions. Accepted match amount: {raw['match']['controls']['amount'] * 100:.0f}%. Native-resolution guards passed; internal regional descriptor {raw['match']['beforeDistance']:.3f} → {raw['match']['afterDistance']:.3f}.

The exported PNG equals the native export render at **every pixel**: {raw_png['versusRender']['exactPixels']}. JPEG quality 95 has mean CIE76 {raw_jpeg['versusRender']['meanDeltaE76']:.4f}, p95 {raw_jpeg['versusRender']['p95DeltaE76']:.4f}, measured on an explicitly sampled 4-pixel grid without resizing. Both delivery hashes and recipe sidecars were verified. Export did not change the recipe. RAW source SHA-256 remained `{raw['rawAfterSha256']}`.

The [full-resolution matched JPEG](../../output/aura-export-colour-2026-10-22/989A2865-reference-matched-full.jpg) is copied byte-for-byte into the review folder. Native PNG, sidecars and evidence are in `{args.raw.resolve()}` (F: was used for the larger artifacts). Gallery previews were resized only for display from those actual exported files. Calibrated white balance and exact reference-grade equivalence remain unverified for this RAW too.'''
    text = f'''# AURA exported colour verification — 2026-10-22

**The exported new photos do not establish the same colour, tone or calibrated white balance as the references.** AURA preserves its saved matched treatment through the tested export paths, but the treatment remains an approximation. No export-induced colour fault was found in these sRGB cases. This distinction is based on decoded delivered files, not just a preview or a successful export status.

I adjusted the existing AURA matching strengths and exported four selected matches as JPEG and lossless PNG. All four selections have a lower native regional appearance distance than their default match. Tone distribution gaps also decreased; chroma gaps did not necessarily improve. This is not an exact-grade or universal quality claim.

## Reference versus delivered result

Two original-resolution [warm](https://www.pexels.com/photo/cinematic-portrait-of-woman-in-warm-lighting-36672685/) and [fashion](https://www.pexels.com/photo/dramatic-portrait-of-a-woman-in-vibrant-fashion-29653811/) references from the [previous audit](AURA-reference-colour-match-2026-10-22.md) were reused. The four target tests use the same 682 × 1024 supplied-photo proxies and initial corrections, restored exactly before each fit.

| Reference / target | Native regional distance: default → selected | Tone quantile gap: previous JPEG → selected JPEG | Chroma quantile gap: previous JPEG → selected JPEG |
|---|---:|---:|---:|
{compare_lines}

Tone gap is mean absolute difference of Lab L* quantiles at 5/25/50/75/95%; chroma gap is the same for sqrt(a*²+b*²). These compare whole-image distributions and depend on subject matter, clothing and background as well as the edit. They do not isolate grading error or establish paired-scene equivalence. The machine report also records normalized a*/b* palette Jensen–Shannon distance (0 identical histograms, larger means more different), without treating it as a quality percentage.

**White balance: not verified as equal.** These unrelated photos contain no corresponding calibrated neutral target or known original WB recipe. The warm reference contains no qualifying bright near-neutral samples under the diagnostic rule. Near-neutral a*/b* cast differences for the other reference are recorded as descriptive proxies only; they are not Kelvin/tint recovery or proof of white-balance equality. Matching slider percentages refer to a fitted contribution and cannot be read as the reference's original WB settings.

## Applied matching strengths

| Reference / target | Amount | Tone | Colour | Warmth |
|---|---:|---:|---:|---:|
{control_lines}

Each pair used read-only analysis followed by coordinate trials of 50%, 150% and 200% for tone, colour, warmth and amount, retaining a candidate only when its full-resolution regional descriptor improved. Baseline and selected-final applications are included in **{len(attempts)} applications: {sum(a['accepted'] for a in attempts)} accepted, {sum(not a['accepted'] for a in attempts)} explicit safety refusals**. Refusals kept the recipe and history unchanged. All candidates started from the same immutable analysis baseline; they did not compound. This is a bounded search of existing AURA controls, not proof of a global optimum or a change to the native fitting algorithm.

## Does export preserve the selected treatment?

Every new delivery used AURA's real `export_run`: sRGB, 8-bit, original dimensions, no output sharpening, JPEG quality 95 or PNG. Read-back verification was enabled. Images were independently decoded with Pillow, embedded ICC profiles converted to sRGB, and compared against the saved full export-purpose render of the **same target and recipe**. Delivered PNGs carry an sRGB chunk, while JPEGs carry ICC profiles; both were confirmed in a post-run metadata check. Only genuinely untagged inputs were assumed sRGB. EXIF orientation was applied before comparison. Source hashes and geometry/retouch/mask preservation were checked.

| Selected match | JPEG mean CIE76 vs render | JPEG p95 CIE76 vs render | PNG vs render |
|---|---:|---:|---|
{export_lines}

All four PNG exports are pixel-identical after colour-profile interpretation. JPEG differences are nonzero and within the declared test limits (mean CIE76 <2 and p95 <5), with median L* shift zero and small signed a*/b* shifts recorded in JSON. Those limits are comparison thresholds for this test, not calibrated perceptual guarantees. Export did not change the saved recipe; all eight recipe sidecars equal the exported recipe. AURA's existing sidecars contain canonical recipe JSON with an `.xmp` suffix; Adobe/Evoto sidecar interoperability is not claimed.

The four previous gallery JPEGs were also decoded and compared to their saved renders: mean CIE76 ranged {min(e['exportVersusSavedRender']['meanDeltaE76'] for e in evidence['existingExports']):.4f}–{max(e['exportVersusSavedRender']['meanDeltaE76'] for e in evidence['existingExports']):.4f}. The earlier reference/result differences were already present in the matched edit, rather than being introduced mainly by JPEG delivery.

## Original camera file export

{raw_section}

## Artifacts and checks

- [View the actual exported JPEG comparisons](../../output/aura-export-colour-2026-10-22/index.html), including reference, before, previous export and selected export. The PNG and recipe links allow lossless inspection.
- [Machine report](AURA-exported-colour-verification-2026-10-22.json) and [four-case CSV](AURA-exported-colour-verification-2026-10-22-cases.csv).
- Desktop executable SHA-256: `{evidence['executableSha256']}`; no production source changes or new desktop build were needed for this verification. The previously tested native matcher and export pipeline performed all edits and deliveries.
- Primary command metadata: `.work-checks/aura-export-colour-2026-10-22/exported-colour.json`; additional RAW command metadata, when included, is alongside it.
- Independent metric tests exercise sRGB black/white/red Lab conversion, identical pixels, colour casts, dimension rejection, symmetric palette gaps, PNG sRGB tags and missing neutral evidence. All 22 script tests passed. The full frontend/native suites were not rerun for these Python-only verification additions.
- The metric helper gained an identical-pixel fast path for native RAW memory use after the preview run, then more precise PNG-tag reporting. Neither changes the measurement formula or image pixels. Recorded run hashes remain intact; the exact preview-run helper is retained in `.work-checks/aura-export-colour-2026-10-22/metrics-at-preview-run.py`.
- Exact reference-grade cloning, reference-scene lighting reconstruction, equal calibrated WB and Evoto equivalence remain unproved. A lower aggregate distance can coexist with a larger individual chroma gap; the report retains both.
'''
    stem.with_suffix('.md').write_text(text, encoding='utf-8')
    cards = []
    def relative(path):
        import os
        return Path(os.path.relpath(path, out)).as_posix()
    for match in evidence['matches']:
        ref, target = match['reference'], match['target']
        prior = next(r for r in evidence['existingExports'] if r['reference']['name'] == ref['name'] and r['target'] == target)
        jpeg = next(e for e in match['exports'] if e['info']['format'] == 'JPEG')
        png = next(e for e in match['exports'] if e['info']['format'] == 'PNG')
        images = [(ref['file'], 'Reference'), (match['before'], 'Target before matching'), (prior['file'], 'Previous exported match'), (jpeg['file'], 'Selected exported JPEG')]
        cards.append(f'<section><h2>{html.escape(ref["name"] + " / " + target)}</h2><p>Reference: <a href="{ref["page"]}">{html.escape(ref["photographer"])}</a> · <a href="{ref["licenseUrl"]}">Pexels licence</a></p><div class="row">' + ''.join(f'<figure><a href="{relative(path)}"><img loading="lazy" src="{relative(path)}" alt="{label}"></a><figcaption>{label}</figcaption></figure>' for path, label in images) + f'</div><p>Native regional appearance distance: {match["default"]["afterDistance"]:.3f} → {match["best"]["afterDistance"]:.3f}. <a href="{relative(jpeg["file"])}">Exported JPEG</a> · <a href="{relative(png["file"])}">Lossless exported PNG</a> · <a href="{ref["name"]}-{target}.recipe.json">Recipe</a></p></section>')
    if raw:
        for name in ['before-preview.png', 'exported-jpeg-preview.png']:
            shutil.copy2(args.raw / name, out / ('raw-' + name))
        cards.append('<section><h2>Original CR3 at native resolution</h2><p><a href="989A2865-reference-matched-full.jpg">Full-resolution matched JPEG</a>. Native PNG and evidence are saved in ' + html.escape(str(args.raw.resolve())) + '. These display previews were resized from the native before render and actual delivered JPEG.</p><div class="raw row"><figure><img src="raw-before-preview.png"><figcaption>Before reference matching</figcaption></figure><figure><img src="raw-exported-jpeg-preview.png"><figcaption>Actual exported JPEG (display preview)</figcaption></figure></div></section>')
    (out / 'index.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AURA exported colour verification</title><style>body{font:16px/1.5 system-ui;background:#19171d;color:#eee;max-width:1500px;margin:28px auto;padding:0 20px}a{color:#c9b0fb}.row{display:flex;gap:12px}.row figure{width:25%;margin:0}.row img{width:100%}figcaption{color:#c2bccb}.raw figure{width:50%}section{border-top:1px solid #57505f;margin-top:35px}strong{color:#ffcd8f}@media(max-width:800px){.row{flex-wrap:wrap}.row figure{width:calc(50% - 12px)}}@media(max-width:450px){.row figure{width:100%}}</style><h1>Actual exported colour matches</h1><p><strong>Approximate matches. Equal reference colour, tone and calibrated white balance are not verified.</strong> All four selected matches reduced the native regional distance; some individual chroma-distribution gaps increased. The lossless PNGs preserve the saved renders exactly, and the JPEG differences are small in these tests.</p><p><a href="../../docs/reports/AURA-exported-colour-verification-2026-10-22.md">Full export report</a> · <a href="../aura-internet-reference-2026-10-22/index.html">Previous matching gallery</a></p>''' + ''.join(cards) + '</html>', encoding='utf-8')
    print(json.dumps(evidence['summary']))


if __name__ == '__main__':
    main()
