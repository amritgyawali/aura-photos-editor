"""Exercise preview failure/retry and undo/redo in an isolated native test catalog.

Run after verify-professional-retouch.py. A simulated unavailable preview affects
both progressive qualities until the hook is restored. Only read-only requests
are intercepted. Undo is reversed by Redo and the recipe hash is checked.
"""
import argparse
import json
from pathlib import Path

from playwright.sync_api import expect, sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--port', type=int, default=9337)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        browser = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
        page = browser.contexts[0].pages[0]
        page.set_default_timeout(180000)
        expect.set_options(timeout=180000)
        page.get_by_role('button', name='Professional retouch verification 1', exact=True).click()
        page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
        retouch = page.get_by_role('button', name='Retouch', exact=True)
        if retouch.count():
            retouch.click()
        show = page.get_by_role('button', name='Show retouched areas', exact=True)
        expect(show).to_be_enabled()
        if show.get_attribute('aria-pressed') != 'true':
            show.click()
        expect(page.get_by_alt_text('Saved retouch coverage')).to_be_visible()
        initial = page.evaluate('''async () => {
            const invoke = window.__TAURI_INTERNALS__.invoke;
            const project = (await invoke('list_projects')).find(p => p.name === 'Professional retouch verification');
            const [photo] = await invoke('list_images', {input: {projectId: project.id, offset: 0, limit: 10, orderBy: null}});
            const recipe = await invoke('image_recipe', {input: {photoId: photo.id}});
            return {photoId: photo.id, recipeHash: recipe.recipeHash};
        }''')
        page.evaluate('''() => {
            // Tauri freezes invoke; intercept only its read-only preview transport.
            window.__auraRecoveryOriginalFetch = window.fetch;
            window.__auraRecoveryOriginalPost = window.chrome.webview.postMessage;
            window.chrome.webview.postMessage = function(message) {
                let data;
                try { data = typeof message === 'string' ? JSON.parse(message) : message; } catch { /* Non-IPC message. */ }
                if (data?.cmd === 'native_retouch_preview') {
                    window.__TAURI_INTERNALS__.runCallback(data.error,
                        {code: 'TEST-PREVIEW', message: 'Simulated preview unavailable', runbookUrl: '', retryable: true});
                    return;
                }
                return window.__auraRecoveryOriginalPost.call(this, message);
            };
            window.fetch = (input, ...args) => {
                const url = typeof input === 'string' ? input : input.url;
                if (url.endsWith('/native_retouch_preview')) {
                    return Promise.resolve(new Response(JSON.stringify({code: 'TEST-PREVIEW', message: 'Simulated preview unavailable', runbookUrl: '', retryable: true}),
                        {headers: {'Content-Type': 'application/json', 'Tauri-Response': 'error'}}));
                }
                return window.__auraRecoveryOriginalFetch.call(window, input, ...args);
            };
        }''')
        try:
            page.get_by_role('button', name='Undo', exact=True).click()
            expect(page.get_by_role('alert').filter(has_text='Simulated preview unavailable')).to_be_visible()
            expect(page.get_by_alt_text('Saved retouch coverage')).to_have_count(0)
            expect(page.get_by_alt_text('Retouched photograph')).to_have_count(0)
            expect(page.get_by_role('button', name='Apply retouch', exact=True)).to_be_disabled()
            expect(page.get_by_role('button', name='Undo', exact=True)).to_be_disabled()
            expect(page.get_by_role('button', name='Back to Develop', exact=True)).to_be_enabled()
            page.screenshot(path=str(args.output / 'preview-failure.png'))
        finally:
            page.evaluate('''() => {
                window.fetch = window.__auraRecoveryOriginalFetch;
                window.chrome.webview.postMessage = window.__auraRecoveryOriginalPost;
                delete window.__auraRecoveryOriginalFetch;
                delete window.__auraRecoveryOriginalPost;
            }''')
        page.get_by_role('button', name='Reload retouch', exact=True).click()
        redo = page.get_by_role('button', name='Redo', exact=True)
        expect(redo).to_be_enabled()
        redo.click()
        expect(page.get_by_alt_text('Saved retouch coverage')).to_be_visible()
        recipe = page.evaluate('(photoId) => window.__TAURI_INTERNALS__.invoke("image_recipe", {input: {photoId}})', initial['photoId'])
        assert recipe['recipeHash'] == initial['recipeHash'], 'Undo/redo did not restore the saved recipe'
        page.screenshot(path=str(args.output / 'preview-recovered.png'))
        result = {'stale_preview_cleared': True, 'edits_blocked_on_failure': True,
                  'reload_recovered': True, 'undo_redo_recipe_preserved': True, **initial}
        (args.output / 'recovery.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
