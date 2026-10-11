"""Check native offline body retouch and early batch cancellation.

Use an isolated native catalogue. AURA produces every edit and export; the
harness hashes inputs and inspects saved recipes without modifying pixels.
"""
import argparse
import hashlib
import json
import shutil
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--port', type=int, default=9363)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
incoming = args.output / 'incoming'
incoming.mkdir(exist_ok=True)
source_copy = incoming / args.source.name
shutil.copyfile(args.source, source_copy)
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
original = digest(args.source)
report = {'page_errors': [], 'original_sha256': original}

with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    page.set_default_timeout(300000)
    page.on('pageerror', lambda error: report['page_errors'].append(str(error)))

    def call(command, value=None):
        return page.evaluate('async ([c,v])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,v)}catch(e){throw new Error(JSON.stringify(e))}}', [command, value or {}])

    name = 'Offline body fallback ' + time.strftime('%H-%M-%S')
    project = call('create_project', {'input': {'name': name, 'coupleNames': None, 'eventDate': None}})['id']
    call('set_cloud_privacy', {'input': {'projectId': project, 'enabled': True, 'offlineStudioMode': True, 'blurFaces': False}})
    call('start_ingest', {'input': {'projectId': project, 'roots': [str(incoming.resolve())]}})
    deadline = time.monotonic() + 300
    while True:
        rows = call('list_images', {'input': {'projectId': project, 'offset': 0, 'limit': 10, 'orderBy': None}})
        if rows:
            break
        assert time.monotonic() < deadline
        time.sleep(.5)
    [photo] = rows
    before = call('image_recipe', {'input': {'photoId': photo['id']}})
    result = call('enhance_portrait', {'input': {'photoId': photo['id']}})
    body = json.loads(result['body'])
    portrait = body['studio_portrait_auto_v1']
    assert portrait['options']['scope'] == 'face_and_body'
    assert not portrait.get('faces'), 'This fixture should have no detectable face'
    edits = body.get('studio_retouch_v1', [])
    assert any(edit['tool'] == 'frequency_heal' for edit in edits), 'Offline default editing skipped visible body skin'
    assert body['global'] == json.loads(before['body'])['global'], 'Retouch changed the existing global grade'
    assert not any(row['source'] == 'cloud' for row in call('cloud_calls', {'projectId': project, 'limit': 100}))
    saved = call('image_recipe', {'input': {'photoId': photo['id']}})
    destination = args.output / 'cancelled-export'
    job = call('one_click_finish', {'input': {'projectId': project, 'destination': str(destination.resolve()), 'ingestJobId': None, 'keepEverything': True}})['jobId']
    assert call('one_click_cancel', {'jobId': job}), 'A running batch did not accept cancellation'
    deadline = time.monotonic() + 120
    while True:
        status = call('one_click_status', {'jobId': job})
        if status['status'] not in ('running', 'cancelling'):
            break
        assert time.monotonic() < deadline
        time.sleep(.5)
    assert status['status'] == 'cancelled' and status['written'] == 0, status
    assert call('image_recipe', {'input': {'photoId': photo['id']}})['recipeHash'] == saved['recipeHash'], 'Early cancellation changed a saved photo'
    # An explicit face-only choice remains authoritative even when its values
    # are identical to the old default options.
    manual_project = call('create_project', {'input': {'name': 'Explicit face-only preference ' + time.strftime('%H-%M-%S'), 'coupleNames': None, 'eventDate': None}})['id']
    call('start_ingest', {'input': {'projectId': manual_project, 'roots': [str(incoming.resolve())]}})
    deadline = time.monotonic() + 300
    while True:
        manual_rows = call('list_images', {'input': {'projectId': manual_project, 'offset': 0, 'limit': 10, 'orderBy': None}})
        if manual_rows:
            break
        assert time.monotonic() < deadline
        time.sleep(.5)
    [manual_photo] = manual_rows
    chosen = call('auto_retouch', {'input': {'projectId': manual_project, 'photoId': manual_photo['id'], 'global': False, 'options': {}}})
    custom = json.loads(chosen['body'])['studio_portrait_auto_v1']['options']
    assert custom['scope'] == 'face'
    manual_result = call('enhance_portrait', {'input': {'photoId': manual_photo['id']}})
    assert json.loads(manual_result['body'])['studio_portrait_auto_v1']['options'] == custom, 'Automatic editing replaced an explicit default-valued face-only choice'
    assert digest(args.source) == digest(source_copy) == original
    assert not report['page_errors']
    report.update({'collection': name, 'project': project, 'photo': photo['id'], 'recipe_hash': saved['recipeHash'],
                   'options': portrait['options'], 'operations': len(edits), 'offline_body_healed': True,
                   'original_unchanged': True, 'cancelled_batch': status, 'early_cancel_preserved_recipe': True,
                   'explicit_default_face_only_preserved': True})
    (args.output / 'verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    call('set_cloud_privacy', {'input': {'projectId': project, 'enabled': True, 'offlineStudioMode': False, 'blurFaces': False}})
    print(json.dumps({'collection': name, 'offline_body_healed': True, 'early_cancel_preserved_recipe': True, 'original_unchanged': True}), flush=True)
