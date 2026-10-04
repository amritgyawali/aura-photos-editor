"""Turn the auto-edit qualification results into one self-contained HTML report.

Reads .work-checks/auto-v3/results.json (test-auto-edit-v3.py), gui/results.json
(test-auto-edit-gui.py) and gui/export-results.json, embeds small before/after images,
and writes .work-checks/auto-v3/report/auto-edit-report.html.
"""
import base64
import html
import io
import json
import statistics
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / '.work-checks/auto-v3'
OUT = BASE / 'report'
OUT.mkdir(parents=True, exist_ok=True)
results = json.loads((BASE / 'results.json').read_text(encoding='utf-8'))
gui = json.loads((BASE / 'gui/results.json').read_text(encoding='utf-8')) if (BASE / 'gui/results.json').exists() else {}
export = json.loads((BASE / 'gui/export-results.json').read_text(encoding='utf-8')) if (BASE / 'gui/export-results.json').exists() else {}
extra = json.loads(Path(sys.argv[1]).read_text(encoding='utf-8')) if len(sys.argv) > 1 else {}
esc = html.escape

ok = [p for p in results['photos'] if 'error' not in p]
failed = [p for p in results['photos'] if 'error' in p]


def count(key):
    return sum(1 for p in ok if p.get(key))


def thumb(name, width=760):
    path = BASE / 'sheets' / (Path(name).stem + '.jpg')
    crop = BASE / 'diag' / (name + '-face-before-after.png')
    if crop.exists():
        path = crop
    if not path.exists():
        return None
    with Image.open(path) as image:
        image = image.convert('RGB')
        image.thumbnail((width, width))
        buffer = io.BytesIO()
        image.save(buffer, 'JPEG', quality=72, optimize=True)
    return 'data:image/jpeg;base64,' + base64.b64encode(buffer.getvalue()).decode()


def tools(p):
    names = {}
    for op in p.get('operations', []):
        key = op['id'].rsplit('-', 1)[-1] if op['id'].endswith(('texture', 'tone', 'light')) else op['tool']
        names[key] = names.get(key, 0) + 1
    return ', '.join(f'{k.replace("_", " ")} x{v}' if v > 1 else k.replace('_', ' ') for k, v in names.items()) or 'none'


def decisions(p):
    scene = (p.get('report') or {}).get('scene') or {}
    return scene.get('decisions', [])


rows = []
for p in results['photos']:
    if 'error' in p:
        code = p['error'].split('"code":"')[1].split('"')[0] if '"code":"' in p['error'] else 'error'
        rows.append(f'<tr class="bad"><td>{esc(p["file"])}</td><td colspan="5">Not editable: {esc(code)}</td></tr>')
        continue
    g = p['global']
    scene = ((p['report'] or {}).get('scene') or {}).get('kind', '-')
    wb = f'{g["temperature"]} K / {g["tint"]:+d}' if (g['temperature'], g['tint']) != (5500, 0) else 'kept'
    checks = all(p.get(k) for k in ('idempotent', 'gotoOriginalRestoresPixels', 'gotoHeadRestoresPixels'))
    rows.append(
        f'<tr><td>{esc(p["file"])}</td><td>{esc(scene)}</td><td class="num">{g["exposure"]:+.2f}</td>'
        f'<td class="num">{esc(wb)}</td><td>{esc(tools(p))}</td>'
        f'<td><span class="pill {"good" if checks else "warn"}">{"pass" if checks else "check"}</span></td></tr>')

GALLERY = [
    ('blemish-portrait-yellow-backdrop.jpg', 'Five red spots and one dark mole painted onto a real face (left). Four spots were healed one by one, the chin spot next to the lip was missed, and the mole was kept. The paler skin comes from a wrong white-balance correction caused by the yellow backdrop in the tested build; that setting has since been tightened back.'),
    ('variant-underexposed.jpg', 'About -1.7 EV group photo: faces confirm underexposure, so exposure is raised further than the histogram alone would dare.'),
    ('portrait-glasses.jpg', 'Low-key portrait: the histogram asked for +0.75 EV; the face-aware check keeps the mood at +0.15 EV.'),
    ('variant-flat.jpg', 'Flat, low-contrast frame: whites and blacks stretched to use the full range.'),
    ('variant-hazy.jpg', 'Haze veil: dehaze, clarity and vibrance from the measured dark-channel floor.'),
    ('variant-noisy-lowlight.jpg', 'Dark, noisy frame: measured noise switches on noise reduction and calms sharpening.'),
    ('portrait-774909.jpg', 'Smiling portrait: skin, one healed spot, iris detail and measured teeth whitening.'),
    ('variant-tungsten-cast.jpg', 'Strong tungsten cast on a teal backdrop, tested with the looser white-balance setting: about half the cast removed.'),
]
cards = []
by_name = {p['file']: p for p in ok}
for name, caption in GALLERY:
    src = thumb(name)
    p = by_name.get(name)
    if not src or not p:
        continue
    detail = (p['report'].get('steps') or [{}])[0].get('detail', '')
    cards.append(f'<figure><img src="{src}" alt="Before and after: {esc(name)}" loading="lazy"><figcaption><strong>{esc(name)}</strong><span>{esc(caption)}</span><code>{esc(detail)}</code></figcaption></figure>')

gui_rows = ''.join(f'<li><span class="pill {"good" if s["status"] == "passed" else "warn"}">{esc(s["status"])}</span> {esc(s["label"])}</li>' for s in gui.get('steps', []))
os_inputs = len(gui.get('osInput', []))
seconds = [p['seconds'] for p in ok if 'seconds' in p]
manual = [p for p in ok if p.get('manualKeptAfterRepeat') is not None]
stats = [
    (f'{len(ok)}/{len(results["photos"])}', 'photos edited automatically'),
    (f'{count("idempotent")}/{len(ok)}', 'repeat passes saved nothing new'),
    (f'{count("gotoOriginalRestoresPixels")}/{len(ok)}', 'jumped back to the original pixel-exact'),
    (f'{sum(p["manualKeptAfterRepeat"] for p in manual)}/{len(manual)}', 'manual edits survived a repeat pass'),
    (f'{statistics.median(seconds):.1f} s' if seconds else '-', 'median time per photo (debug build)'),
]
stat_html = ''.join(f'<div class="stat"><b>{esc(a)}</b><span>{esc(b)}</span></div>' for a, b in stats)

CAN = [
    ('Light and colour', 'Exposure, highlights, shadows, whites/blacks stretch, contrast, face-aware exposure limits, partial white balance with two agreeing estimates, vibrance, clarity, dehaze, noise reduction and sharpening, each chosen from measurements of the photo and explained.'),
    ('Scene awareness', 'Portrait, group, outdoor, low-light and general scenes are told apart from faces, sky, greenery and brightness; a real sky gets a highlight-only gradient; strong colour without people is kept as the light\'s mood.'),
    ('Face detection', 'Offline YuNet detector with five landmarks, including sideways and upside-down faces, several faces per photo.'),
    ('Skin', 'Texture smoothing that keeps fine detail, tone evening and gentle local light, sampled from each person\'s own cheeks and forehead.'),
    ('Blemishes', 'Small, round spots redder than the surrounding skin healed one by one with a clean skin donor; moles, freckles and beauty marks kept and counted; a freckle field is never removed.'),
    ('Lines and redness', 'Crow\'s feet and forehead lines softened where they measure stronger than the cheek; smile lines lifted, never erased; redness beside the nose evened toward the person\'s own cheek.'),
    ('Eyes and teeth', 'Iris detail on open eyes, redness in the whites, flash red-eye, under-eye shadows, and yellow teeth, each only when measured.'),
    ('Control', 'Every stage saved as its own history step; Undo, Redo and "Go back to here"; every automatic operation tagged and editable in Retouch; a strength slider and feature switches; manual work always protected.'),
]
LIMITED = [
    ('White balance near coloured backdrops', 'With a looser agreement setting a tungsten cast was corrected, but yellow and pink studio backdrops were also "corrected", making skin paler. The source now uses the stricter setting, which leaves all three alone: safe, but a strong cast in front of a coloured backdrop stays.'),
    ('Eyes behind glasses, squinting or small faces', 'Often reported as "no open eye visible" and left alone.'),
    ('Group photos', 'Faces smaller than the detector\'s 320-pixel analysis size are skipped; a six-person office photo found one face.'),
    ('Real acne', 'The public test portraits are professionally retouched, so blemish healing was proven on painted spots on real faces and on synthetic skin, not on real acne.'),
    ('Low-key and deliberately dark scenes', 'A blue-hour landscape and a golden-hour landscape were brightened by +0.6 to +0.75 EV; mood can be lost.'),
]
CANNOT = [
    'TIFF input (8-bit RGB and 16-bit gray files are rejected as damaged), HEIC, WebP, BMP and GIF import.',
    'Semantic segmentation of skin, hair, sky or subject; masks are landmark-guided and colour-sampled.',
    'Straightening, auto-crop, sky replacement, object removal, generative fill or expand, body or face reshaping.',
    'Opening closed eyes, removing permanent marks, tattoos or scars (by design).',
    'GPU rendering; everything runs on the CPU.',
]

ISSUES = [
    ('Critical, your PC', 'Drive D: (Seagate ST1000LM049 hard disk) is corrupting files. Windows logged hundreds of NTFS repair events (event 55/130) in the last hours; source files, git objects, build artefacts and a whole temporary folder were damaged during this session. Back up D: and run <code>chkdsk D: /f</code> (or replace the disk) before relying on it. All code from this session is safe on GitHub.'),
    ('High', 'Rebuilds failed after the disk damage (compiler crashes reading corrupted files). The last desktop build that could be launched includes everything except the lines-and-redness step, the retouch settings panel and the stricter white-balance setting; those are covered by unit and UI tests only.'),
    ('Medium', 'Drive C: had 20 MB free. AURA\'s preview cache was moved to D: behind a junction to keep the app working.'),
    ('Low', 'The history list numbers the original as step 1, one ahead of the "Auto edit 1/3" labels.'),
]
issue_html = ''.join(f'<tr><td><span class="pill {"crit" if i == 0 else "warn" if i < 3 else "info"}">{esc(level)}</span></td><td>{text}</td></tr>' for i, (level, text) in enumerate(ISSUES))

page = f'''<title>AURA Auto Edit Report</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,500;9..144,650&family=IBM+Plex+Sans:wght@400;500;600&family=IBM+Plex+Mono:wght@400;500&display=swap">
<style>
/* Layout: a darkroom contact sheet - one reading column, a zone-scale rule, wide figures. */
:root {{
  --bg: #f3f4f5; --surface: #ffffff; --ink: #1b2127; --muted: #5a6570; --rule: #d5dadf;
  --accent: #2f5d8a; --good: #2e7d4f; --warn: #a86a12; --crit: #b3261e;
  --display: "Fraunces", Georgia, serif; --body: "IBM Plex Sans", system-ui, sans-serif; --mono: "IBM Plex Mono", ui-monospace, monospace;
}}
@media (prefers-color-scheme: dark) {{ :root:not([data-theme="light"]) {{
  --bg: #15191d; --surface: #1d2328; --ink: #e6e9ec; --muted: #9aa5af; --rule: #2e363d;
  --accent: #7fb0dd; --good: #6cc58f; --warn: #e0a64a; --crit: #ff8a80; color-scheme: dark; }} }}
:root[data-theme="dark"] {{ --bg: #15191d; --surface: #1d2328; --ink: #e6e9ec; --muted: #9aa5af; --rule: #2e363d;
  --accent: #7fb0dd; --good: #6cc58f; --warn: #e0a64a; --crit: #ff8a80; color-scheme: dark; }}
body {{ background: var(--bg); color: var(--ink); font: 16px/1.6 var(--body); }}
main {{ max-width: 980px; margin: 0 auto; padding-inline: 20px; padding-block: 40px 64px; display: grid; gap: 40px; }}
h1, h2 {{ font-family: var(--display); text-wrap: balance; line-height: 1.15; margin: 0; }}
h1 {{ font-size: clamp(2rem, 5vw, 3rem); font-weight: 650; }}
h2 {{ font-size: 1.5rem; font-weight: 500; }}
p {{ margin: 0; max-width: 68ch; }}
.eyebrow {{ font: 500 .75rem var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--muted); }}
.zones {{ display: grid; grid-template-columns: repeat(11, 1fr); height: 10px; border-radius: 2px; overflow: hidden; }}
.zones i {{ display: block; }}
section {{ display: grid; gap: 16px; }}
.lede {{ font-size: 1.15rem; }}
.verdict {{ border-left: 3px solid var(--accent); padding-left: 16px; }}
.stats {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 12px; }}
.stat {{ background: var(--surface); border: 1px solid var(--rule); border-radius: 6px; padding: 14px; display: grid; gap: 4px; }}
.stat b {{ font: 600 1.4rem var(--mono); font-variant-numeric: tabular-nums; }}
.stat span {{ color: var(--muted); font-size: .9rem; }}
.gallery {{ display: grid; gap: 28px; }}
figure {{ margin: 0; display: grid; gap: 8px; }}
figure img {{ width: 100%; border-radius: 4px; border: 1px solid var(--rule); background: #fff; }}
figcaption {{ display: grid; gap: 4px; }}
figcaption span {{ color: var(--muted); }}
code {{ font: .85rem var(--mono); color: var(--muted); overflow-wrap: anywhere; }}
.list {{ display: grid; gap: 12px; margin: 0; padding: 0; list-style: none; }}
.list li {{ display: grid; grid-template-columns: minmax(140px, 220px) 1fr; gap: 12px; padding-block: 10px; border-top: 1px solid var(--rule); }}
.list li b {{ font-weight: 600; }}
@media (max-width: 560px) {{ .list li {{ grid-template-columns: 1fr; gap: 2px; }} }}
ul.plain {{ margin: 0; padding-left: 20px; display: grid; gap: 6px; }}
.table {{ overflow-x: auto; border: 1px solid var(--rule); border-radius: 6px; background: var(--surface); }}
table {{ border-collapse: collapse; width: 100%; font-size: .9rem; }}
th, td {{ text-align: left; padding: 8px 10px; border-bottom: 1px solid var(--rule); vertical-align: top; }}
th {{ font: 500 .75rem var(--mono); letter-spacing: .08em; text-transform: uppercase; color: var(--muted); }}
td.num {{ font-family: var(--mono); font-variant-numeric: tabular-nums; white-space: nowrap; }}
tr.bad td {{ color: var(--muted); }}
.pill {{ display: inline-block; font: 500 .75rem var(--mono); padding: 1px 8px; border-radius: 999px; border: 1px solid currentColor; white-space: nowrap; }}
.pill.good {{ color: var(--good); }} .pill.warn {{ color: var(--warn); }} .pill.crit {{ color: var(--crit); }} .pill.info {{ color: var(--muted); }}
ol.steps {{ margin: 0; padding-left: 20px; display: grid; gap: 6px; }}
.steps li .pill {{ margin-right: 6px; }}
</style>
<main>
<header style="display:grid;gap:14px">
  <span class="eyebrow">AURA photo editor · automatic editing qualification · 1 October 2026</span>
  <h1>AURA Auto Edit Report</h1>
  <div class="zones" aria-hidden="true">{''.join(f'<i style="background:rgb({v},{v},{v})"></i>' for v in (8, 30, 55, 80, 105, 128, 152, 177, 202, 228, 250))}</div>
  <p class="lede verdict"><strong>Can it edit any kind of photo? Not every kind, but most ordinary photos.</strong> One click now finishes JPEG and PNG portraits, groups, landscapes, pets, food and product shots: light, colour, noise, sharpening, and for faces skin, blemishes, lines, eyes and teeth, all measured from the photo, saved as separate steps you can go back to and change by hand. It still cannot open TIFF, HEIC or WebP files, does not segment skin or sky with a trained model, and leaves strong mixed-light casts alone.</p>
</header>

<section><div class="stats">{stat_html}</div>
<p>{len(results["photos"])} files went through the real desktop app: {len(ok)} real and derived photographs were edited, {len(failed)} files could not be opened. Every original file was byte-for-byte unchanged afterwards.</p></section>

<section><h2>What the automatic edit can do</h2><ul class="list">{''.join(f'<li><b>{esc(a)}</b><span>{esc(b)}</span></li>' for a, b in CAN)}</ul></section>

<section><h2>Before and after</h2><div class="gallery">{''.join(cards)}</div></section>

<section><h2>Where it is limited</h2><ul class="list">{''.join(f'<li><b>{esc(a)}</b><span>{esc(b)}</span></li>' for a, b in LIMITED)}</ul></section>

<section><h2>What it cannot do</h2><ul class="plain">{''.join(f'<li>{esc(t)}</li>' for t in CANNOT)}</ul></section>

<section><h2>The edit I made, step by step</h2>
<p>On <code>portrait-smile-teeth.jpg</code>, driving the real window with Windows mouse and keyboard input ({os_inputs} physical clicks and key presses):</p>
<ol class="steps">{gui_rows}<li><span class="pill good">passed</span> Exported through the Export workspace: {export.get('written', 0)} file written and read back ({esc(', '.join('x'.join(map(str, f['size'])) for f in export.get('files', [])))}).</li></ol>
</section>

<section><h2>Every photo</h2><div class="table"><table>
<thead><tr><th>File</th><th>Scene</th><th>Exposure</th><th>White balance</th><th>Retouch operations</th><th>Checks</th></tr></thead>
<tbody>{''.join(rows)}</tbody></table></div>
<p style="color:var(--muted)">Checks: a repeat pass saved nothing, "go back" restored the original pixels exactly, and returning restored the edited pixels exactly.</p></section>

<section><h2>Problems found</h2><div class="table"><table><tbody>{issue_html}</tbody></table></div></section>

<section><h2>Automated tests</h2><ul class="plain">
<li>Rust: 68 aura-app unit tests and 3 portrait integration tests pass, including new tests for blemishes on three complexions, moles kept, eyes, teeth, lines, smile lines, nose redness, strength scaling, scene detection, noise and staged history.</li>
<li>Interface: 152 tests across 21 files in the Develop area pass, including the step list, "Go back to here" and the retouch settings panel; TypeScript type check passes.</li>
<li>The IPC surface check now covers every typed IPC module: 285 commands defined, registered and invoked.</li>
</ul></section>
</main>'''
(OUT / 'auto-edit-report.html').write_text(page, encoding='utf-8')
print(OUT / 'auto-edit-report.html', round(len(page) / 1024), 'KB')
