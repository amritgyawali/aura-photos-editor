"""The selected effects (ADR-0131 to ADR-0135) and the Effects screen in the real desktop window,
on a real photograph.

Starts the desktop app against an isolated catalogue with WebView2 CDP, imports one photograph,
opens Retouch, and:

- creates one effect through the window itself: picks it in the Tool list, sets the target
  region with the numeric fields and clicks Apply, then checks the saved operation;
- then, for every effect, saves exactly that one operation through the same command the Apply
  button calls, renders the photograph, and checks that the effect changed pixels inside its
  region and none far outside it (the drop shadow is allowed to fall beside its subject);
- writes a contact sheet of every effect for a person to look at, and checks the page raised
  no errors.

    python scripts/test-effects-gui.py PHOTO [--port 9372]

`AURA_EXE` names the build (default `target/desktop-launch/debug/aura-desktop.exe`, a debug
build so `AURA_TEST_CATALOG` is honoured). Writes `.work-checks/effects-gui/`.
"""
import argparse
import base64
import json
import re
import os
import shutil
import subprocess
import time
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/effects-gui'

# amount, tone, warmth, tint, texture, radius, feather - as `ui/src/ipc/retouchEffects.ts`.
DEFAULTS = {
    'liquify_push': (.5, .5, .3, 0, 1, .01, .6), 'liquify_bloat': (.35, .5, 0, 0, 1, .01, .6),
    'liquify_pucker': (.35, .5, 0, 0, 1, .01, .6), 'liquify_twirl': (.5, .5, .3, 0, 1, .01, .6),
    'spin_blur': (1, .5, 0, 0, 1, .01, .4), 'pixelate': (1, .5, 0, 0, 1, .02, 0),
    'privacy_blur': (1, .5, 0, 0, 1, .02, .2), 'soft_focus': (.5, .3, 0, 0, 1, .015, .5),
    'orton_glow': (.5, .4, 0, 0, 1, .02, .5), 'local_sharpen': (.7, .5, 0, 0, .8, .002, .5),
    'local_denoise': (.8, .5, 0, 0, 1, .004, .5),
    'hue_shift': (1, .5, .25, 0, 1, .01, .4), 'invert_colors': (1, .5, 0, 0, 1, .01, .2),
    'chroma_key': (1, .4, 0, 0, 1, .01, .2), 'vein_reduce': (.7, .5, 0, 0, 1, .008, .5),
    'color_unify': (.5, .5, 0, 0, 1, .01, .4), 'local_white_balance': (1, .5, 0, 0, 1, .01, .6),
    'light_leak': (.6, .5, 0, 0, 1, .01, .8), 'glitch': (1, .4, 0, 0, 1, .006, .1),
    'haze': (.5, .5, 0, 0, 1, .01, .8), 'starburst': (.6, .5, 0, 0, 1, .01, .8),
    'halation': (.6, .5, 0, 0, 1, .01, .5), 'catchlight': (.8, .5, 0, 0, 1, .01, .7),
    'eye_glow_fix': (1, .6, 0, 0, 1, .01, .3), 'lip_gloss': (.6, .5, 0, 0, 1, .004, .5),
    'freckles': (.7, .5, 0, 0, 1, .004, .6), 'floor_reflection': (.5, .4, 0, 0, 1, .01, .1),
    'drop_shadow': (.6, .5, .4, .4, 1, .015, .2),
    'split_tone': (0.8, 0.5, 0, 0, 1, 0.01, 0.4),
    'film_fade': (0.8, 0.4, 0.3, 0, 1, 0.01, 0.4),
    'bleach_bypass': (0.8, 0.5, 0, 0, 1, 0.01, 0.4),
    'teal_orange': (0.8, 0.5, 0, 0, 1, 0.01, 0.4),
    'colour_splash': (1, 0.3, 0, 0, 1, 0.01, 0.3),
    'sky_blue': (0.8, 0.5, 0, 0, 1, 0.01, 0.6),
    'spotlight': (0.9, 0.5, 0, 0, 1, 0.015, 0.5),
    'rim_light': (0.8, 0.5, 0.7, -0.4, 1, 0.006, 0.2),
    'chromatic_aberration': (1, 0.5, 0, 0, 1, 0.02, 0.4),
    'fisheye': (0.6, 0.5, 0, 0, 1, 0.01, 0.4),
    'anamorphic_streak': (0.8, 0.5, 0, 0, 1, 0.02, 0.4),
    'kaleidoscope': (1, 0.5, 0, 0, 1, 0.01, 0.3),
    'watercolor': (1, 0.5, 0, 0, 1, 0.004, 0.3),
    'stained_glass': (1, 0.5, 0, 0, 1, 0.02, 0.2),
    'cartoon': (1, 0.4, 0, 0, 1, 0.003, 0.2),
    'neon_edges': (1, 0.6, 0, 0, 1, 0.01, 0.2),
    'thermal': (1, 0.5, 0, 0, 1, 0.01, 0.2),
    'clipped_highlight_repair': (1, 0.5, 0, 0, 1, 0.01, 0.4),
    'local_dehaze': (0.8, 0.6, 0, 0, 1, 0.01, 0.5),
    'limbal_ring': (0.6, 0.4, 0, 0, 1, 0.01, 0.2),
    # ADR-0135.
    'lash_darken': (0.6, 0.5, 0, 0, 1, 0.002, 0.3),
    'hair_fill': (0.7, 0.6, 0, 0, 1, 0.01, 0.3),
    'match_grain': (1, 0.5, 0, 0, 1, 0.01, 0.3),
    'channel_swap': (1, 0.5, 0, 0, 1, 0.01, 0.3),
    'sunset_glow': (0.8, 0.5, 0, 0, 1, 0.01, 0.4),
    'day_to_night': (1, 0.5, 0, 0, 1, 0.01, 0.3),
    'lomo': (0.8, 0.6, 0, 0, 1, 0.01, 0.3),
    'instant_film': (0.8, 0.5, 0, 0, 1, 0.01, 0.3),
    'pop_art': (1, 0.5, 0, 0, 1, 0.01, 0.1),
    'iris_blur': (1, 0.3, 0, 0, 1, 0.006, 0.2),
    'echo_double': (0.7, 0.5, 0.08, 0, 1, 0.01, 0.3),
    'ripple': (0.6, 0.5, 0, 0, 1, 0.008, 0.3),
    'frosted_glass': (1, 0.5, 0, 0, 1, 0.004, 0.3),
    'pixel_sort': (1, 0.5, 0, 0, 1, 0.01, 0.1),
    'vhs_retro': (0.9, 0.5, 0, 0, 1, 0.01, 0.2),
    'emboss': (1, 0.2, 0, 0, 1, 0.01, 0.2),
    'crosshatch': (1, 0.5, 0, 0, 1, 0.004, 0.2),
    'sparkle': (0.8, 0.6, 0, 0, 1, 0.008, 0.2),
    'fairy_lights': (0.8, 0.5, 0, 0, 1, 0.01, 0.3),
    'rainbow_prism': (0.6, 0.5, 0.15, 0, 1, 0.02, 0.4),
}
COLOURS = {'chroma_key': [.2, .45, .85], 'light_leak': [1, .55, .25], 'haze': [.82, .84, .87],
           'starburst': [1, .97, .9], 'catchlight': [1, 1, 1]}
# Tools whose change is subtle on a photograph that has nothing for them to fix: they only
# have to leave everything outside their region alone.
QUIET = {'clipped_highlight_repair', 'sky_blue', 'anamorphic_streak', 'eye_glow_fix', 'vein_reduce', 'local_white_balance', 'chroma_key', 'lip_gloss', 'halation',
         # Darken only what is darker than its surroundings, or star only real glints (ADR-0135).
         'lash_darken', 'sparkle'}


def tool_ids():
    """Every retouch tool the window offers, read from the window's own source."""
    text = (ROOT / 'ui/src/ipc/nativeRetouch.ts').read_text(encoding='utf-8')
    base = re.findall(r"^\s*\['([a-z_]+)', '", text.split('export const RETOUCH_TOOLS = [')[1].split('] as const')[0], re.M)
    effects = re.findall(r"^\s*\['([a-z_]+)', '", (ROOT / 'ui/src/ipc/retouchEffects.ts').read_text(encoding='utf-8'), re.M)
    return base + [e for e in effects if e not in base]


def launch(exe, port):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'),
               AURA_TEST_CATALOG=str(OUT / 'catalog' / 'catalog.sqlite'))
    (OUT / 'catalog').mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen([str(exe)], cwd=str(ROOT), env=env)
    # Wait for this app's own page, not merely for a debugging port: another AURA on the machine
    # must never be driven by mistake.
    for _ in range(180):
        if proc.poll() is not None:
            raise RuntimeError(f'AURA exited during start-up (code {proc.returncode})')
        try:
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/list', timeout=1) as reply:
                pages = json.loads(reply.read())
            if any(p.get('type') == 'page' and 'tauri' in p.get('url', '') for p in pages):
                return proc
        except Exception:  # noqa: BLE001 - the port is not open until the window is
            pass
        time.sleep(1)
    proc.terminate()
    raise RuntimeError('AURA did not start')


def pixels(dto):
    return np.asarray(Image.frombytes('RGB', (dto['width'], dto['height']), base64.b64decode(dto['rgbBase64'])),
                      dtype=np.int16)


def operation(tool, region):
    amount, tone, warmth, tint, texture, radius, feather = DEFAULTS[tool]
    source = None
    if tool in ('chroma_key', 'local_white_balance', 'colour_splash'):
        source = [region[0], region[1]]
    return {'id': 'draft', 'tool': tool, 'enabled': True, 'region': region, 'source': source,
            'amount': amount, 'feather': feather, 'radius': radius, 'texture': texture, 'tone': tone,
            'warmth': warmth, 'tint': tint, 'targetColor': COLOURS.get(tool)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('photo', type=Path)
    parser.add_argument('--port', type=int, default=9372)
    args = parser.parse_args()
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    if OUT.exists():
        shutil.rmtree(OUT)
    sources = OUT / 'sources'
    sources.mkdir(parents=True)
    shutil.copy2(args.photo, sources / args.photo.name)
    report = {'exe': str(exe), 'checks': {}, 'effects': {}, 'pageErrors': []}
    proc = launch(exe, args.port)
    time.sleep(2)
    assert (OUT / 'catalog' / 'catalog.sqlite').exists(), 'the catalogue is not isolated - stop'

    def check(name, ok, detail=''):
        report['checks'][name] = {'ok': bool(ok), 'detail': detail}
        print(('PASS ' if ok else 'FAIL ') + name + (f' - {detail}' if detail else ''))

    try:
        with sync_playwright() as pw:
            # WebView2 swaps processes when it navigates to the app's page; a connection made just
            # before the swap stays attached to a blank target, so reconnect until the page is seen.
            page = None
            for _ in range(8):
                browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
                for _ in range(15):
                    found = [p for c in browser.contexts for p in c.pages]
                    page = next((p for p in found if 'localhost' in p.url or 'tauri' in p.url), None)
                    if page:
                        break
                    time.sleep(1)
                if page:
                    break
                browser.close()
            if page is None:
                raise RuntimeError(f'no AURA page on port {args.port}')
            page.set_default_timeout(300000)
            expect.set_options(timeout=300000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',
                                     [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            name = 'Effects ' + time.strftime('%H-%M-%S')
            project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(sources)])
            photos, deadline = [], time.monotonic() + 300
            while not photos and time.monotonic() < deadline:
                photos = call('list_images', projectId=project, offset=0, limit=10, orderBy='timeline')
                time.sleep(0.5)
            photo = photos[0]['id']

            def stack(action, edits=(), op_id=None):
                return raw('native_retouch_edit', input=dict(projectId=project, photoId=photo, action=action,
                                                             edits=list(edits), id=op_id))

            # Open Retouch in the window.
            page.reload()
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.get_by_role('button', name=f'{name} 1', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            option = page.get_by_role('option', name=photos[0]['fileName'], exact=True)
            for _ in range(6):
                option.click()
                time.sleep(1.5)
                if option.get_attribute('aria-selected') == 'true':
                    break
            retouch = page.get_by_role('button', name='Retouch', exact=True)
            expect(retouch).to_be_enabled()
            retouch.click()
            expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()

            # One effect made entirely through the window.
            page.get_by_label('Tool', exact=True).select_option('liquify_bloat')
            expect(page.get_by_text('Enlarges what is at the centre of the ellipse')).to_be_visible()
            for label, value in [('Center X (%)', '50'), ('Center Y (%)', '40'),
                                 ('Horizontal radius (%)', '12'), ('Vertical radius (%)', '9')]:
                page.get_by_label(label, exact=True).fill(value)
            apply = page.get_by_role('button', name='Apply retouch', exact=True)
            expect(apply).to_be_enabled()
            apply.click()
            deadline = time.monotonic() + 300
            saved = []
            while not saved and time.monotonic() < deadline:
                saved = stack('list')
                time.sleep(1)
            check('an effect chosen and applied in the window is saved',
                  len(saved) == 1 and saved[0]['tool'] == 'liquify_bloat'
                  and abs(saved[0]['region'][0] - 0.5) < 1e-3 and abs(saved[0]['region'][2] - 0.12) < 1e-3,
                  json.dumps(saved[0] if saved else None)[:200])
            page.screenshot(path=str(OUT / '01-window-effect.png'), full_page=True)

            # Every effect, one at a time, on the photograph.
            stack('clear')
            before = pixels(raw('native_retouch_preview', projectId=project, photoId=photo, before=True,
                                quality='fast'))
            h, w = before.shape[:2]
            tiles = []
            for tool in DEFAULTS:
                region = [0.5, 0.42, 0.16, 0.12]
                if tool == 'floor_reflection':
                    region = [0.5, 0.82, 0.3, 0.12]
                if tool == 'drop_shadow':
                    region = [0.45, 0.4, 0.08, 0.1]
                stack('clear')
                stack('append', [operation(tool, region)])
                after = pixels(raw('native_retouch_preview', projectId=project, photoId=photo, before=False,
                                   quality='fast'))
                ys, xs = np.mgrid[0:h, 0:w]
                d = np.hypot(((xs + 0.5) / w - region[0]) / region[2], ((ys + 0.5) / h - region[1]) / region[3])
                inside = d <= 1.0
                far = d > 1.15
                diff = np.abs(after - before).max(axis=2)
                changed_inside = int((diff[inside] > 2).sum())
                changed_far = int((diff[far] > 2).sum())
                report['effects'][tool] = {'changedInside': changed_inside, 'changedFar': changed_far,
                                           'insidePixels': int(inside.sum())}
                moved = changed_inside > 0 or tool in QUIET
                contained = changed_far == 0 or tool == 'drop_shadow'
                check(f'{tool} renders in its region only', moved and contained,
                      f'{changed_inside} changed inside, {changed_far} far outside')
                x0 = max(0, int((region[0] - region[2] * 1.4) * w))
                x1 = min(w, int((region[0] + region[2] * 1.4) * w))
                y0 = max(0, int((region[1] - region[3] * 1.4) * h))
                y1 = min(h, int((region[1] + region[3] * 1.4) * h))
                tile = Image.fromarray(after[y0:y1, x0:x1].astype(np.uint8)).resize((240, 180))
                ImageDraw.Draw(tile).rectangle([0, 0, 240, 16], fill=(0, 0, 0))
                ImageDraw.Draw(tile).text((4, 2), tool, fill=(255, 255, 255))
                tiles.append(tile)
            region = [0.5, 0.42, 0.16, 0.12]
            x0, x1 = int((region[0] - region[2] * 1.4) * w), int((region[0] + region[2] * 1.4) * w)
            y0, y1 = int((region[1] - region[3] * 1.4) * h), int((region[1] + region[3] * 1.4) * h)
            original = Image.fromarray(before[y0:y1, x0:x1].astype(np.uint8)).resize((240, 180))
            ImageDraw.Draw(original).rectangle([0, 0, 240, 16], fill=(0, 0, 0))
            ImageDraw.Draw(original).text((4, 2), 'original', fill=(255, 255, 255))
            tiles.insert(0, original)
            cols = 6
            sheet = Image.new('RGB', (cols * 244, ((len(tiles) + cols - 1) // cols) * 184), (24, 24, 28))
            for i, tile in enumerate(tiles):
                sheet.paste(tile, ((i % cols) * 244 + 2, (i // cols) * 184 + 2))
            sheet.save(OUT / 'contact-sheet.png')
            stack('clear')

            # The Effects screen (ADR-0135): find a tool by what it does and open it on this photo.
            page.reload()
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.get_by_role('button', name=f'{name} 1', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
            page.get_by_role('button', name='Effects Every retouch tool & creative look', exact=True).click()
            library = page.get_by_role('region', name='Effects library')
            expect(library).to_be_visible()
            uses = library.get_by_role('button', name='Use on this photo', exact=True)
            total = uses.count()
            expected = len(tool_ids())
            check('the Effects screen lists every tool', total == expected and uses.first.is_enabled(),
                  f'{total} tools, {expected} in the tool lists')
            page.screenshot(path=str(OUT / '02-effects-screen.png'), full_page=False)
            page.get_by_label('Search tools', exact=True).fill('jewellery')
            card = library.get_by_role('listitem').filter(has=page.get_by_role('heading', name='Sparkle', exact=True))
            expect(card).to_have_count(1)
            check('search finds a tool by what it is for', uses.count() < 10, f'{uses.count()} shown')
            card.get_by_role('button', name='Use on this photo', exact=True).click()
            tool = page.get_by_label('Tool', exact=True)
            expect(tool).to_have_value('sparkle')
            for label, value in [('Center X (%)', '50'), ('Center Y (%)', '42'),
                                 ('Horizontal radius (%)', '16'), ('Vertical radius (%)', '12')]:
                page.get_by_label(label, exact=True).fill(value)
            apply = page.get_by_role('button', name='Apply retouch', exact=True)
            expect(apply).to_be_enabled()
            apply.click()
            deadline, saved = time.monotonic() + 300, []
            while not saved and time.monotonic() < deadline:
                saved = stack('list')
                time.sleep(1)
            check('Use on this photo opens Retouch with that tool, and it saves',
                  len(saved) == 1 and saved[0]['tool'] == 'sparkle', json.dumps(saved[0] if saved else None)[:160])
            page.screenshot(path=str(OUT / '03-from-effects.png'), full_page=False)
            stack('clear')
            check('the page raised no errors', not report['pageErrors'], '; '.join(report['pageErrors']))
    finally:
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        proc.terminate()
    failed = [k for k, v in report['checks'].items() if not v['ok']]
    print(f"{len(report['checks']) - len(failed)} of {len(report['checks'])} checks passed")
    raise SystemExit(1 if failed else 0)


if __name__ == '__main__':
    main()
