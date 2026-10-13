"""Verify internal AI batch editing through a running isolated native AURA app.

The app imports, grades, retouches, exports and reads its output back. This
harness reads results and hashes originals; it never processes final pixels.
Use --custom-first to verify that saved per-photo retouch choices survive.
"""
import argparse
import hashlib
import json
import shutil
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('sources', type=Path, nargs='+')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--port', type=int, default=9363)
parser.add_argument('--custom-first', action='store_true')
parser.add_argument('--offline', action='store_true', help='verify local batch editing without image requests')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
incoming = args.output / 'incoming'
incoming.mkdir(exist_ok=True)
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
originals = []
for index, source in enumerate(args.sources):
    target = incoming / f'{index + 1}-{source.name}'
    shutil.copyfile(source, target)
    originals.append({'path': str(source.resolve()), 'copy': str(target.resolve()), 'sha256': digest(source)})
report = {'originals': originals, 'status': 'running', 'page_errors': []}

def save():
    (args.output / 'verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')

with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    page.set_default_timeout(300000)
    page.on('pageerror', lambda error: report['page_errors'].append(str(error)))

    def call(command, value=None):
        return page.evaluate('async ([c,v])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,v)}catch(e){throw new Error(JSON.stringify(e))}}', [command, value or {}])

    project = call('create_project', {'input': {'name': 'Internal AI batch verification ' + time.strftime('%H-%M-%S'), 'coupleNames': None, 'eventDate': None}})['id']
    report['project'] = project
    ingest = call('start_ingest', {'input': {'projectId': project, 'roots': [str(incoming.resolve())]}})
    deadline = time.monotonic() + 300
    rows = []
    while len(rows) < len(originals):
        assert time.monotonic() < deadline, 'Import did not finish'
        rows = call('list_images', {'input': {'projectId': project, 'offset': 0, 'limit': 100, 'orderBy': None}})
        time.sleep(.5)
    call('set_cloud_privacy', {'input': {'projectId': project, 'enabled': True, 'offlineStudioMode': args.offline, 'blurFaces': False}})
    custom_id = None
    custom = None
    if args.custom_first:
        custom_id = next(row['id'] for row in rows if row['fileName'] == Path(originals[0]['copy']).name)
        chosen = call('auto_retouch', {'input': {'projectId': project, 'photoId': custom_id, 'global': False,
            'options': {'scope': 'face_and_body', 'adaptive': False, 'eyes': False, 'teeth': False, 'refine': False,
                        'settings': {'deepBlemishCleanup': True, 'cleanupPasses': 4, 'frequencyHeal': .75, 'protectEyeArea': True, 'protectNoseDetail': True}}}})
        custom = json.loads(chosen['body'])['studio_portrait_auto_v1']['options']
    destination = args.output / 'export'
    job = call('one_click_finish', {'input': {'projectId': project, 'destination': str(destination.resolve()), 'ingestJobId': None, 'keepEverything': True}})['jobId']
    report['job'] = job
    deadline = time.monotonic() + 1800
    previous = None
    while True:
        status = call('one_click_status', {'jobId': job})
        report['batch'] = status
        save()
        if status != previous:
            print(json.dumps({k: status[k] for k in ('status', 'phase', 'phaseLabel', 'itemsDone', 'itemsTotal', 'aiEdited', 'localEdited', 'failedEdits', 'written', 'verified')}), flush=True)
            previous = status
        if status['status'] not in ('running', 'cancelling'):
            break
        assert time.monotonic() < deadline, 'Batch did not finish in 30 minutes'
        time.sleep(2)
    assert status['status'] in ('completed', 'completed_with_issues'), status
    assert status['failedEdits'] == 0 and status['written'] == status['verified'] == len(originals), status
    report['photos'] = []
    for row in rows:
        recipe = call('image_recipe', {'input': {'photoId': row['id']}})
        body = json.loads(recipe['body'])
        portrait = body.get('studio_portrait_auto_v1', {})
        assert portrait.get('options', {}).get('scope') == 'face_and_body', 'Batch skipped default body-skin scope'
        assert body.get('studio_retouch_v1'), 'A skin fixture received no saved retouch operations'
        report['photos'].append({'id': row['id'], 'file': row['fileName'], 'recipe_hash': recipe['recipeHash'],
                                'global': body['global'], 'options': portrait.get('options'),
                                'operations': len(body.get('studio_retouch_v1', [])), 'message': portrait.get('message')})
        if row['id'] == custom_id:
            assert portrait['options'] == custom, 'Batch replaced saved custom retouch choices'
    calls = call('cloud_calls', {'projectId': project, 'limit': 100})
    if args.offline:
        assert status['aiEdited'] == 0 and not any(row['source'] == 'cloud' for row in calls), 'Offline batch made an image request'
        report['offline_no_cloud_calls'] = True
    report['ai_calls'] = [{k: row.get(k) for k in ('id', 'task', 'source', 'status', 'model', 'decisionRef', 'estimatedCostUsd')} for row in calls]
    report['custom_settings_preserved'] = custom_id is not None
    for original in originals:
        assert digest(Path(original['path'])) == digest(Path(original['copy'])) == original['sha256']
    assert not report['page_errors']
    report['originals_unchanged'] = True
    report['status'] = 'verified'
    save()
    page.screenshot(path=str(args.output / 'native-batch.png'))
    print(json.dumps({'verified': True, 'photos': len(rows), 'ai_edited': status['aiEdited'], 'local_edited': status['localEdited'], 'originals_unchanged': True, 'custom_settings_preserved': custom_id is not None}), flush=True)
