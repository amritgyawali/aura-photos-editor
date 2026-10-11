"""Exercise live internal AI and read-only residual review in an isolated native app.

AURA imports, edits and renders. This harness only drives its UI/IPC and reads
evidence. It never prints keys or stores the credential file in the evidence.
"""
import argparse
import base64
import hashlib
import json
import re
import shutil
from pathlib import Path

import numpy as np
from playwright.sync_api import expect, sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--port', type=int, default=9363)
parser.add_argument('--collection', default='Internal AI verification')
parser.add_argument('--resume', action='store_true', help='reuse this harness collection after a failed check')
parser.add_argument('--reset', action='store_true', help='reset only this isolated test photo before checking a fresh edit')
parser.add_argument('--config', type=Path, default=Path(r'D:\AI-Company\HERMES-AI-CONFIG.md'))
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
incoming = args.output / 'incoming'
incoming.mkdir(exist_ok=True)
original_hash = hashlib.sha256(args.source.read_bytes()).hexdigest()
copy = incoming / args.source.name
shutil.copyfile(args.source, copy)
report = {'original_sha256': original_hash, 'page_errors': []}

def no_secrets(value):
    encoded = json.dumps(value)
    for line in args.config.read_text(encoding='utf8').splitlines():
        field, _, value = line.partition('=')
        if field.strip().endswith('_API_KEY'):
            secret = value.strip().strip('"\'')
            if len(secret) > 7:
                assert secret not in encoded, 'Sensitive data appeared in safe evidence'

def save_report():
    no_secrets(report)
    (args.output / 'verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')

with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    page.set_default_timeout(300000)
    expect.set_options(timeout=300000)
    page.on('pageerror', lambda e: report['page_errors'].append(str(e)))
    def call(command, value=None):
        return page.evaluate('async ([c,v])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,v)}catch(e){throw new Error(JSON.stringify(e))}}', [command, value or {}])
    profiles = call('internal_ai_status', {'refresh': False})
    no_secrets(profiles)
    assert profiles['sourceAvailable'] and len(profiles['profiles']) >= 11
    report['profiles'] = profiles
    # A first-run modal may have loaded before background configuration finished.
    skip = page.get_by_role('button', name='Not now - AURA works without this', exact=True)
    if skip.count():
        skip.click()
    if not args.resume:
        page.get_by_label('New collection', exact=True).fill(args.collection)
        page.get_by_role('button', name='Create', exact=True).click()
        page.locator('.studio-nav').get_by_role('button', name='Photos Browse your collection', exact=True).click()
        page.get_by_text('Enter folders manually', exact=True).click()
        page.get_by_label('Card or folder', exact=True).fill(str(incoming.resolve()))
        page.get_by_role('button', name='Add folder', exact=True).click()
        page.get_by_role('button', name='Start import', exact=True).click()
    else:
        page.get_by_role('button', name=f'{args.collection} 1', exact=True).click()
        page.locator('.studio-nav').get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
    expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
    project = next(v for v in call('list_projects') if v['name'] == args.collection)
    [photo] = call('list_images', {'input': {'projectId': project['id'], 'offset': 0, 'limit': 10, 'orderBy': None}})
    value = {'projectId': project['id'], 'photoId': photo['id']}
    if args.reset:
        call('history_step', {'input': {**value, 'action': 'reset_original'}})
    def recipe():
        return call('image_recipe', {'input': {'photoId': photo['id']}})
    def pixels():
        frame = call('native_retouch_preview', {**value, 'before': False, 'quality': 'full'})
        return np.frombuffer(base64.b64decode(frame['rgbBase64']), dtype=np.uint8).reshape(frame['height'], frame['width'], 3).copy()
    call('set_cloud_privacy', {'input': {'projectId': project['id'], 'enabled': True, 'offlineStudioMode': False, 'blurFaces': False}})
    before = recipe()
    before_pixels = pixels()
    report['before_grade_recipe_hash'] = before['recipeHash']
    answer = call('photo_auto_edit', {'input': {**value, 'jobId': 'internal-ai-grade-verification'}})
    no_secrets(answer)
    report['grade'] = {'source': answer['source'], 'model': answer['model'], 'reasons': answer['reasons']}
    save_report()
    assert answer['source'] in ('cloud', 'cache'), 'No validated live AI grade; inspect provider status and audit'
    after = recipe()
    after_pixels = pixels()
    report['after_grade_recipe_hash'] = after['recipeHash']
    if before['recipeHash'] != after['recipeHash']:
        call('history_step', {'input': {**value, 'action': 'undo'}})
        report['after_undo_recipe_hash'] = recipe()['recipeHash']
        save_report()
        assert report['after_undo_recipe_hash'] == before['recipeHash'] and np.array_equal(pixels(), before_pixels)
        call('history_step', {'input': {**value, 'action': 'redo'}})
        assert recipe()['recipeHash'] == after['recipeHash'] and np.array_equal(pixels(), after_pixels)
    report['grade_history_exact'] = True
    repeated = call('photo_auto_edit', {'input': {**value, 'jobId': 'internal-ai-repeat-verification'}})
    assert repeated['recipe']['recipeHash'] == recipe()['recipeHash'] == after['recipeHash'], 'An unchanged AI recommendation changed the saved recipe'
    assert np.array_equal(pixels(), after_pixels)
    report['repeat_grade_preserves_saved_recipe'] = True
    retouched = call('enhance_portrait', {'input': {'photoId': photo['id']}})
    body = json.loads(retouched['body'])
    report['retouch_options'] = body['studio_portrait_auto_v1']['options']
    if not args.resume or args.reset:
        assert report['retouch_options']['scope'] == 'face_and_body'
        assert report['retouch_options']['settings']['bodyBlemishes'] > 0, 'Fresh AI guidance disabled visible body healing'
    report['retouch_operations'] = len(body.get('studio_retouch_v1', []))
    calls = call('cloud_calls', {'projectId': project['id'], 'limit': 100})
    no_secrets(calls)
    report['calls'] = calls
    save_report()
    skin = [c for c in calls if c['task'] == 'native_skin_advice']
    assert skin, 'Skin advice was not evaluated on this fresh default-settings photo'
    report['skin_advice_sources'] = [c['source'] for c in skin]
    successful = next(c for c in calls if c['source'] == 'cloud' and c['status'] in ('ok', 'repaired'))
    for accepted in (False, True):
        call('internal_ai_feedback', {'projectId': project['id'], 'callId': successful['id'], 'accepted': accepted})
    refreshed = call('cloud_calls', {'projectId': project['id'], 'limit': 100})
    feedback = [c for c in refreshed if c['task'] == 'internal_ai_feedback']
    assert len(feedback) == 1 and feedback[0]['status'] == 'accepted'
    report['feedback_replaces_same_rating'] = True
    save_report()
    saved = recipe()
    edits = json.loads(saved['body']).get('studio_retouch_v1', [])
    selection = next(e for e in edits if e['tool'] in ('frequency_heal', 'acne_clear'))
    review = call('native_retouch_blemish_review', {'input': {**value, 'recipeHash': saved['recipeHash'], 'selection': selection, 'quality': 'full'}})
    assert recipe()['recipeHash'] == saved['recipeHash']
    report['residual_review'] = review
    report['review_read_only'] = True
    save_report()
    try:
        call('native_retouch_blemish_review', {'input': {**value, 'recipeHash': 'stale', 'selection': selection, 'quality': 'full'}})
        raise AssertionError('A stale review was accepted')
    except Exception as error:
        assert 'saved photo changed' in str(error).lower()
    report['stale_review_refused'] = True
    call('set_cloud_privacy', {'input': {'projectId': project['id'], 'enabled': True, 'offlineStudioMode': True, 'blurFaces': False}})
    offline = call('photo_auto_edit', {'input': {**value, 'jobId': 'internal-ai-offline-verification'}})
    assert offline['source'] == 'local_fallback'
    call('internal_ai_status', {'refresh': True})
    assert call('cloud_status')['offlineStudioMode']
    report['offline_refresh_does_not_reenable_ai'] = True
    call('set_cloud_privacy', {'input': {'projectId': project['id'], 'enabled': True, 'offlineStudioMode': False, 'blurFaces': False}})
    assert hashlib.sha256(args.source.read_bytes()).hexdigest() == original_hash
    assert hashlib.sha256(copy.read_bytes()).hexdigest() == original_hash
    report['original_unchanged'] = True
    report['imported_copy_unchanged'] = True
    assert not report['page_errors']
    no_secrets(report)
    save_report()
    page.screenshot(path=str(args.output / 'native-ai.png'))
    print(json.dumps({'grade': report['grade']['source'], 'model': report['grade']['model'], 'skin_advice': report['skin_advice_sources'], 'profiles': len(profiles['profiles']), 'original_unchanged': True}), flush=True)
