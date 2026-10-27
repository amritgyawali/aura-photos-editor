"""Verify status-read recovery on a real native folder run in an isolated test window.

Connect to an already launched AURA test window with --port. Input photos are copied
into the report directory. Only status reads are faulted; import, editing, cancellation
and export still use the native engine. Do not use a window containing unsaved user work.
"""
import argparse
import hashlib
import json
import re
import shutil
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect, sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('sources', type=Path, nargs='+')
    parser.add_argument('--port', type=int, default=9375)
    args = parser.parse_args()
    output = args.output.resolve()
    incoming = output / 'incoming'
    incoming.mkdir(parents=True, exist_ok=True)
    hashes = {}
    for index, source in enumerate(args.sources):
        copied = incoming / f'{index}-{source.name}'
        shutil.copyfile(source, copied)
        hashes[str(copied)] = hashlib.sha256(copied.read_bytes()).hexdigest()
    report = {'page_errors': []}
    with sync_playwright() as playwright:
        browser = playwright.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
        page = browser.contexts[0].pages[0]
        page.set_default_timeout(60000)
        expect.set_options(timeout=60000)
        page.on('pageerror', lambda error: report['page_errors'].append(str(error)))
        page.locator('.studio-nav').get_by_role('button', name=re.compile('^Start')).click()
        if page.locator('.finish-folder details').get_attribute('open') is None:
            page.get_by_text('Folder and export location', exact=True).click()
        page.get_by_label('Folder to finish', exact=True).fill(str(incoming))
        page.get_by_label(re.compile('^Export into')).fill(str(output / 'exports'))
        page.get_by_role('checkbox', name=re.compile('^Cull first')).uncheck()
        page.evaluate('''() => {
            if (window.__runRecovery?.originalFetch) window.fetch = window.__runRecovery.originalFetch;
            if (window.__runRecovery?.originalPost) window.chrome.webview.postMessage = window.__runRecovery.originalPost;
            const native = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
            const originalFetch = window.fetch.bind(window);
            const originalPost = window.chrome.webview.postMessage.bind(window.chrome.webview);
            window.__runRecovery = { native, originalFetch, originalPost, failures: 0, jobId: null, calls: [], reads: new Map() };
            const fault = (jobId, callbackId) => {
                const audit = window.__runRecovery;
                const key = String(callbackId);
                // A transport fallback may send the same logical read over both adapters.
                if (audit.reads.has(key)) return audit.reads.get(key);
                audit.jobId = jobId;
                audit.calls.push(performance.now());
                const fail = audit.failures++ < 3;
                audit.reads.set(key, fail);
                return fail;
            };
            const outage = {code: 'AURA-JOB-7005', message: 'Injected status-read outage', retryable: true};
            // Tauri's public adapter is immutable. Inject an ordinary error response into
            // the WebView fetch transport, preserving the real adapter and its callbacks.
            window.fetch = async (input, options) => {
                if (String(input).endsWith('/one_click_status')) {
                    if (fault(JSON.parse(options.body).jobId, options.headers.get('Tauri-Error'))) return new Response(JSON.stringify(outage),
                        {headers: {'Content-Type': 'application/json', 'Tauri-Response': 'error'}});
                }
                return originalFetch(input, options);
            };
            // The Windows WebView can use its standard message bridge instead of fetch.
            window.chrome.webview.postMessage = input => {
                const message = typeof input === 'string' ? JSON.parse(input) : input;
                if (message.cmd === 'one_click_status' && fault(message.payload.jobId, message.error)) {
                    queueMicrotask(() => window.__TAURI_INTERNALS__.runCallback(message.error, outage));
                    return;
                }
                return originalPost(input);
            };
        }''')
        page.get_by_role('button', name='Finish a whole folder', exact=True).click()
        panel = page.get_by_role('region', name='Automatic processing')
        expect(panel.get_by_text(re.compile('Status checks are paused'))).to_be_visible()
        expect(panel.get_by_role('button', name='Stop automatic processing')).to_be_enabled()
        assert page.evaluate('window.__runRecovery.calls.length') == 3
        assert page.evaluate('JSON.parse(sessionStorage.getItem("aura-automatic-run")).state.jobId === window.__runRecovery.jobId')
        expect(page.locator('.studio-nav').get_by_role('button', name=re.compile('^Auto edit'))).to_be_disabled()
        page.screenshot(path=str(output / 'paused.png'))
        print('PASS three failed reads retain the run, Stop control and busy lock', flush=True)
        # Read the real worker through the saved native adapter before restoring monitoring.
        report['native_during_outage'] = page.evaluate('''async () =>
            await window.__runRecovery.native('one_click_status', {jobId: window.__runRecovery.jobId})''')
        panel.get_by_role('button', name='Retry status').click()
        expect(panel.get_by_role('button', name='Retry status')).to_have_count(0)
        expect(panel.get_by_role('button', name='Stop automatic processing')).to_have_count(0, timeout=300000)
        report['final'] = page.evaluate('''async () =>
            await window.__runRecovery.native('one_click_status', {jobId: window.__runRecovery.jobId})''')
        final = report['final']
        assert final['status'] == 'completed', final
        assert final['verified'] == len(args.sources), final
        assert final.get('failedEdits', 0) == 0, final
        exports = list(Path(final['destination']).rglob('*.jpg'))
        assert len(exports) == len(args.sources)
        report['export_dimensions'] = {}
        for exported in exports:
            with Image.open(exported) as image:
                image.load()
                report['export_dimensions'][exported.name] = list(image.size)
        report['originals_unchanged'] = all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest
                                            for path, digest in hashes.items())
        assert report['originals_unchanged'] and not report['page_errors']
        report['status_calls'] = page.evaluate('window.__runRecovery.calls')
        page.screenshot(path=str(output / 'completed.png'))
        (output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        print(f'PASS recovered and completed {final["verified"]} verified exports; originals unchanged', flush=True)
        # Restore the test-only adapter before leaving the window for other checks.
        page.evaluate('''() => {
            window.fetch = window.__runRecovery.originalFetch;
            window.chrome.webview.postMessage = window.__runRecovery.originalPost;
        }''')
        browser.close()


if __name__ == '__main__':
    main()
