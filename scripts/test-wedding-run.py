"""Finish a whole folder in the real desktop app, and score what it did. ADR-0088.

Starts the desktop build with an isolated catalog and WebView profile (so a photographer's
own catalog and open draft are never touched), presses the real "Finish a whole folder"
button with a genuine Windows mouse click, and follows the native run - import, cull, edit,
retouch, export - to the end. The folder dialog is the one thing that cannot be automated,
so the script answers it with the folder given on the command line and sends the export to
the output folder instead of Pictures.

It records how long each phase took, the peak memory of the application, and every count
the run reported. Given the manifest written by a ground-truth generator it also scores the
cull (what it rejected against what was really wrong with each frame) and the edit (how far
each delivered photograph is from its unspoiled original, before and after).

    python scripts/test-wedding-run.py <folder> <output> [--manifest m.json --truth dir]

All pixel work is done by AURA. This script only reads the files it wrote.
"""
import argparse
import ctypes
import json
import os
import subprocess
import time
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
user32 = ctypes.windll.user32
user32.SetProcessDPIAware()


class POINT(ctypes.Structure):
    _fields_ = [('x', ctypes.c_long), ('y', ctypes.c_long)]


def os_click(page, locator):
    """A real mouse click on the real window, where the control is on screen."""
    locator.scroll_into_view_if_needed()
    box = locator.bounding_box()
    ratio = page.evaluate('devicePixelRatio')
    window = user32.FindWindowW(None, 'AURA')
    if not window or not box:
        return False
    user32.ShowWindow(window, 9)
    user32.SetForegroundWindow(window)
    time.sleep(0.4)
    origin = POINT(0, 0)
    user32.ClientToScreen(window, ctypes.byref(origin))
    user32.SetCursorPos(int(origin.x + (box['x'] + box['width'] / 2) * ratio),
                        int(origin.y + (box['y'] + box['height'] / 2) * ratio))
    time.sleep(0.15)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.05)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(0.4)
    return True


def working_set_mb(pid):
    try:
        row = subprocess.check_output(['tasklist', '/FI', f'PID eq {pid}', '/FO', 'CSV', '/NH'], text=True)
        return int(row.strip().split('","')[-1].strip('" K\n').replace(',', '').replace('.', '')) / 1024
    except Exception:  # noqa: BLE001
        return 0.0


def lab_mean(path):
    """Mean CIELAB of a photograph, from a 64 px average: its overall light and colour."""
    image = Image.open(path).convert('RGB').resize((64, 64), Image.BOX)
    rgb = np.asarray(image, dtype=np.float64) / 255
    linear = np.where(rgb <= 0.04045, rgb / 12.92, ((rgb + 0.055) / 1.055) ** 2.4)
    xyz = linear @ np.array([[0.4124, 0.2126, 0.0193], [0.3576, 0.7152, 0.1192], [0.1805, 0.0722, 0.9505]])
    xyz = xyz / np.array([0.95047, 1.0, 1.08883])
    f = np.where(xyz > 0.008856, np.cbrt(xyz), 7.787 * xyz + 16 / 116)
    lab = np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], axis=-1)
    return lab.reshape(-1, 3).mean(axis=0)


def structure(path):
    """A 24 px luminance thumbnail, normalised: is this export the photograph we think it is?"""
    luma = np.asarray(Image.open(path).convert('L').resize((24, 24), Image.BOX), dtype=np.float64).ravel()
    luma = luma - luma.mean()
    return luma / (np.linalg.norm(luma) + 1e-9)


def median(values):
    return round(float(np.median(values)), 2) if len(values) else None


def score_cull(manifest, verdicts):
    by_file = {row['file']: row for row in manifest}
    kept = {v['fileName']: v['keep'] for v in verdicts}
    reason = {v['fileName']: v['reason'] for v in verdicts}
    result = {'failures': {}, 'bursts': {}}
    for kind in ('defocus', 'motion', 'black', 'blown'):
        files = [f for f, row in by_file.items() if row['kind'] == kind and f in kept]
        rejected = [f for f in files if not kept[f]]
        result['failures'][kind] = {'frames': len(files), 'leftOut': len(rejected),
                                    'missed': sorted(set(files) - set(rejected))[:12]}
    singles = [f for f, row in by_file.items() if row['kind'] == 'normal' and row['burst'] == 1 and f in kept]
    wrong = [f for f in singles if not kept[f]]
    result['goodSingles'] = {'frames': len(singles), 'wronglyLeftOut': len(wrong),
                             'examples': [{'file': f, 'reason': reason[f]} for f in wrong[:12]]}
    moments = {}
    for row in manifest:
        if row['kind'] == 'normal' and row['burst'] > 1 and row['file'] in kept:
            moments.setdefault(row['moment'], []).append(row)
    exact = best = lost = extra = 0
    for rows in moments.values():
        survivors = [row for row in rows if kept[row['file']]]
        allowed = 1 if len(rows) <= 5 else 2
        exact += len(survivors) == allowed
        lost += len(survivors) == 0
        extra += max(0, len(survivors) - allowed)
        best += any(row['best'] for row in survivors)
    result['bursts'] = {'bursts': len(moments), 'reducedToOne': exact, 'sharpestKept': best,
                        'momentsLostEntirely': lost, 'extraFramesKept': extra}
    return result


def score_edits(manifest, source, truth, exports):
    """Distance from each delivered frame to its unspoiled original, before and after."""
    by_file = {row['file']: row for row in manifest}
    groups = {'exposureError': [], 'colourCast': [], 'alreadyGood': [], 'all': []}
    mismatched = 0
    for name, exported in exports.items():
        row = by_file.get(name)
        if not row or row['kind'] != 'normal' or not row.get('truth'):
            continue
        reference = truth / row['truth']
        if not reference.exists():
            continue
        if float(structure(source / name) @ structure(exported)) < 0.6:
            mismatched += 1
            continue
        target, before, after = lab_mean(reference), lab_mean(source / name), lab_mean(exported)
        item = {'file': name, 'ev': row['ev'], 'cast': row['cast'],
                'before': float(np.linalg.norm(before - target)), 'after': float(np.linalg.norm(after - target)),
                'lightBefore': float(abs(before[0] - target[0])), 'lightAfter': float(abs(after[0] - target[0])),
                'colourBefore': float(np.linalg.norm(before[1:] - target[1:])),
                'colourAfter': float(np.linalg.norm(after[1:] - target[1:]))}
        groups['all'].append(item)
        if abs(row['ev']) > 0.6:
            groups['exposureError'].append(item)
        if row['cast'] != 'none':
            groups['colourCast'].append(item)
        if abs(row['ev']) < 0.25 and row['cast'] == 'none':
            groups['alreadyGood'].append(item)
    summary = {'exportsThatDidNotMatchTheirSource': mismatched}
    for label, items in groups.items():
        summary[label] = {
            'frames': len(items),
            'medianDistanceBefore': median([i['before'] for i in items]),
            'medianDistanceAfter': median([i['after'] for i in items]),
            'medianLightErrorBefore': median([i['lightBefore'] for i in items]),
            'medianLightErrorAfter': median([i['lightAfter'] for i in items]),
            'medianColourErrorBefore': median([i['colourBefore'] for i in items]),
            'medianColourErrorAfter': median([i['colourAfter'] for i in items]),
            'closerAfter': sum(1 for i in items if i['after'] < i['before']),
            'fartherByMoreThan5': sum(1 for i in items if i['after'] > i['before'] + 5),
        }
    worst = sorted(groups['all'], key=lambda i: i['after'] - i['before'], reverse=True)[:10]
    summary['movedFarthestFromOriginal'] = [{k: (round(v, 2) if isinstance(v, float) else v) for k, v in i.items()} for i in worst]
    return summary


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('folder', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--truth', type=Path)
    parser.add_argument('--exe', type=Path, default=Path(os.environ.get('AURA_EXE', r'C:\Users\amrit\aura-c-target\debug\aura-desktop.exe')))
    parser.add_argument('--port', type=int, default=9341)
    parser.add_argument('--keep-everything', action='store_true')
    parser.add_argument('--timeout-min', type=float, default=720)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    folder = args.folder.resolve()
    destination = out / 'export'
    sources = sorted(p for p in folder.iterdir() if p.suffix.lower() in ('.jpg', '.jpeg', '.png'))
    report = {'folder': str(folder), 'sourceFiles': len(sources), 'exe': str(args.exe), 'phases': [], 'checks': {}}
    env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}',
               WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'), AURA_TEST_CATALOG=str(out / 'catalog' / 'catalog.sqlite'))
    (out / 'catalog').mkdir(exist_ok=True)
    app = subprocess.Popen([str(args.exe)], cwd=str(ROOT), env=env)
    try:
        for _ in range(240):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1)
                break
            except Exception:  # noqa: BLE001
                time.sleep(1)
        else:
            raise RuntimeError('AURA did not start')
        with sync_playwright() as playwright:
            browser = playwright.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
            page = browser.contexts[0].pages[0]
            page.set_default_timeout(120000)
            errors = []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.locator('.aura-studio').wait_for()
            report['checks']['studioNavigation'] = page.locator('.studio-nav strong').all_text_contents()
            report['checks']['brand'] = page.locator('.studio-brand').inner_text().replace('\n', ' ')
            button = page.get_by_role('button', name='Finish a whole folder', exact=True)
            button.wait_for()
            cull_box = page.get_by_role('checkbox')
            if args.keep_everything and cull_box.is_checked():
                cull_box.uncheck()
            report['checks']['cullSwitchedOn'] = cull_box.is_checked()
            # The native folder dialog cannot be driven, so the two paths are typed into the
            # fields the studio offers for exactly that; everything else is the real control.
            page.get_by_text('Folder and export location', exact=True).click()
            page.get_by_label('Folder to finish').fill(str(folder))
            page.get_by_label('Export into').fill(str(destination))
            button.scroll_into_view_if_needed()
            page.screenshot(path=str(out / 'start.png'))
            started = time.monotonic()
            report['checks']['pressedWithRealMouse'] = os_click(page, button)
            adopted = '() => JSON.parse(sessionStorage.getItem("aura-automatic-run") || "{}").state?.jobId'
            try:
                page.wait_for_function(adopted, timeout=15000)
            except Exception:  # noqa: BLE001
                report['checks']['pressedWithRealMouse'] = False
                button.click()
                page.wait_for_function(adopted, timeout=60000)
            run = page.evaluate('() => JSON.parse(sessionStorage.getItem("aura-automatic-run")).state')
            report['run'] = {'jobId': run['jobId'], 'projectId': run['projectId']}
            print('started', run['jobId'], flush=True)
            status, phase, phase_started, peak, shot = {}, None, started, 0.0, False
            deadline = started + args.timeout_min * 60
            while time.monotonic() < deadline:
                status = page.evaluate('async (job) => await window.__TAURI_INTERNALS__.invoke("one_click_status", { jobId: job })', run['jobId'])
                peak = max(peak, working_set_mb(app.pid))
                now = time.monotonic()
                label = f"{status['phase']}: {status['phaseLabel'].split('.')[0][:70]}" if status['phase'] != 'edit' else 'edit'
                if label != phase:
                    if phase is not None:
                        report['phases'].append({'phase': phase, 'seconds': round(now - phase_started, 1)})
                    print(f"{now - started:8.0f}s  {label}", flush=True)
                    phase, phase_started = label, now
                if status['phase'] == 'edit' and not shot and status['itemsDone'] >= 2:
                    page.screenshot(path=str(out / 'progress.png'))
                    report['checks']['progressPanel'] = page.locator('.automatic-progress').inner_text()[:600]
                    shot = True
                if status['status'] not in ('running', 'cancelling'):
                    report['phases'].append({'phase': phase, 'seconds': round(now - phase_started, 1)})
                    break
                if status['phase'] == 'edit' and status['itemsDone'] % 100 == 0 and status['itemsDone']:
                    print(f"{now - started:8.0f}s  edited {status['itemsDone']}/{status['itemsTotal']}  memory {peak:.0f} MB", flush=True)
                time.sleep(2)
            total = time.monotonic() - started
            report['status'] = status
            report['seconds'] = round(total, 1)
            report['peakWorkingSetMb'] = round(peak)
            report['pageErrors'] = errors
            time.sleep(2)
            page.screenshot(path=str(out / 'done.png'))
            report['checks']['finalPanel'] = page.locator('.automatic-progress').inner_text()[:900]
            rows, offset = [], 0
            while True:
                chunk = page.evaluate('async (a) => await window.__TAURI_INTERNALS__.invoke("list_images", { input: a })',
                                      {'projectId': run['projectId'], 'offset': offset, 'limit': 240, 'orderBy': 'timeline'})
                rows += chunk
                offset += len(chunk)
                if len(chunk) < 240:
                    break
            browser.close()
    finally:
        app.terminate()
        try:
            app.wait(timeout=20)
        except Exception:  # noqa: BLE001
            app.kill()

    names = {row['id']: row['fileName'] for row in rows}
    destination = Path(status.get('destination') or destination)
    report['destination'] = str(destination)
    report['imported'] = len(rows)
    cull_path = destination / 'photo-cull.json'
    cull = json.loads(cull_path.read_text(encoding='utf-8')) if cull_path.exists() else None
    delivered = [v['fileName'] for v in cull['verdicts'] if v['keep']] if cull else [row['fileName'] for row in rows]
    manifest_path = destination / 'aura-delivery-manifest.json'
    exported = json.loads(manifest_path.read_text(encoding='utf-8')) if manifest_path.exists() else {'files': []}
    files = sorted(destination / f['path'] for f in exported['files'])
    exports = dict(zip(delivered, files)) if len(files) == len(delivered) else {}
    edits_path = destination / 'photo-edits.json'
    edits = json.loads(edits_path.read_text(encoding='utf-8')) if edits_path.exists() else []
    faces = operations = 0
    for entry in edits:
        portrait = json.loads(entry['recipe']['body']).get('studio_portrait_auto_v1', {})
        faces += int(portrait.get('detectedFaces', 0) or 0)
        operations += int(portrait.get('operations', 0) or 0)
    report['delivery'] = {
        'delivered': len(delivered), 'filesWritten': len(files), 'manifestVerified': exported.get('verified'),
        'editReportEntries': len(edits), 'editReportMatchesDelivery': [e['photoId'] for e in edits] == [i for i, n in names.items() if n in set(delivered)] if edits else None,
        'facesDetected': faces, 'retouchOperations': operations,
        'megabytes': round(sum(f.stat().st_size for f in files) / 1e6, 1),
        'secondsPerDeliveredPhoto': round(report['seconds'] / max(1, len(files)), 2),
        'secondsPerSourcePhoto': round(report['seconds'] / max(1, len(sources)), 2),
    }
    if cull:
        report['cull'] = {'counts': cull['counts'], 'thresholds': cull['thresholds']}
    if args.manifest and args.manifest.exists():
        manifest = json.loads(args.manifest.read_text(encoding='utf-8'))
        if cull:
            report['cullAgainstGroundTruth'] = score_cull(manifest, cull['verdicts'])
        if args.truth and exports:
            report['editAgainstOriginals'] = score_edits(manifest, folder, args.truth, exports)
    (out / 'wedding-run-report.json').write_text(json.dumps(report, indent=1), encoding='utf-8')
    brief = {k: report[k] for k in ('sourceFiles', 'imported', 'seconds', 'peakWorkingSetMb', 'delivery') if k in report}
    brief['status'] = {k: status.get(k) for k in ('status', 'frames', 'analyzed', 'selected', 'localEdited', 'aiEdited', 'failedEdits', 'written', 'verified')}
    brief['notes'] = status.get('notes')
    print(json.dumps(brief, indent=1))


if __name__ == '__main__':
    main()
