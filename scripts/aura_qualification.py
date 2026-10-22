"""Fail-closed evidence checks shared by desktop photo qualification scripts."""
from pathlib import Path
import html


def separate_output(source: Path, output: Path) -> None:
    source, output = source.resolve(), output.resolve()
    if source == output or source in output.parents or output in source.parents:
        raise ValueError('Qualification output must be separate from the source folder')
    if (output / 'results.json').exists() or (output / 'catalog/catalog.sqlite').exists():
        raise ValueError('Choose a fresh output folder so earlier results cannot qualify this build')


def exact_import(sources, photos) -> None:
    expected = sorted(path.name for path in sources)
    actual = sorted(photo['fileName'] for photo in photos)
    if actual != expected or len({photo['id'] for photo in photos}) != len(expected):
        raise ValueError('Imported identities do not match the complete source inventory')


def photo_success(report) -> bool:
    photos = report.get('photos', [])
    expected = len(report.get('inputManifest', []))
    if not expected or len(photos) != expected or not all(p.get('passed') for p in photos):
        return False
    if not report.get('originalsUnchanged') or report.get('pageErrors') or report.get('fatalError'):
        return False
    if not report.get('fullExportRequested'):
        return True
    export = report.get('export', {})
    files = report.get('exportFiles', [])
    return (
        export.get('requested') == expected
        and export.get('written') == expected
        and export.get('verified') == expected
        and export.get('manifestSealed') is True
        and all(export.get(key) == 0 for key in ('unverified', 'corrupt', 'renderFailed'))
        and len(files) == expected
        and len({Path(p['file']).stem for p in files}) == expected
        and {Path(p['file']).stem for p in files} == {Path(p['file']).stem for p in photos}
    )


def write_gallery(output: Path, report) -> None:
    cards = []
    exports = {Path(p['file']).stem: Path(p['file']) for p in report.get('exportFiles', [])}
    for photo in report.get('photos', []):
        if not photo.get('passed'):
            continue
        name = photo['file']
        export = exports.get(Path(name).stem)
        target = export.relative_to(output).as_posix() if export else name + '-after.jpg'
        cards.append(
            '<article><h2>' + html.escape(name) + '</h2><div>'
            '<figure><img loading="lazy" src="' + html.escape(name + '-before.jpg', quote=True)
            + '"><figcaption>Before</figcaption></figure><figure><a href="'
            + html.escape(target, quote=True) + '"><img loading="lazy" src="'
            + html.escape(name + '-after.jpg', quote=True)
            + '"></a><figcaption>AURA edit; click for export</figcaption></figure></div></article>'
        )
    variant_link = '<p><a href="screenshot-guided-variant/index.html">Review the separate screenshot-guided colour approximation</a></p>' if report.get('screenshotGuidedVariant') else ''
    (output / 'index.html').write_text(
        '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" '
        'content="width=device-width,initial-scale=1"><title>AURA photo review</title>'
        '<style>body{background:#15161a;color:#eee;font:16px system-ui;margin:24px}'
        'article{margin:32px 0}article>div{display:flex;gap:16px}figure{margin:0;flex:1;min-width:0}'
        'img{width:100%;max-height:78vh;object-fit:contain;background:#222}figcaption{text-align:center}'
        '@media(max-width:650px){article>div{display:block}}h2{font-size:18px}</style>'
        '<h1>AURA photo review</h1><p>Saved automatic edits. Originals are preserved. '
        'Exact Evoto equivalence is unverified.</p>' + variant_link + ''.join(cards) + '</html>', encoding='utf-8'
    )
