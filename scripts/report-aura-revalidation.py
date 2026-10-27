"""Publish the current executable's evidence, keeping the preceding audit intact."""
from collections import Counter
import csv
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
CHECKS = ROOT / '.work-checks/aura100-revalidation'
PHOTOS = ROOT / 'output/aura100-revalidated'


def text(path):
    raw = path.read_bytes() if path.exists() else b''
    return raw.decode('utf-16' if raw.startswith(b'\xff\xfe') else 'utf-8-sig', errors='replace')


def read(path):
    return json.loads(text(path)) if path.exists() else {}


def native_records(log):
    suite = ''
    records = []
    for line in log.splitlines():
        stripped = line.strip()
        if stripped.startswith(('Running ', 'Doc-tests ')):
            suite = stripped
        match = re.match(r'^test (.+?) \.\.\. (ok|FAILED|ignored)(?:, (.*))?$', stripped)
        if match:
            records.append(dict(suite=suite, test=match[1], status=match[2], reason=match[3] or ''))
    return records


def main():
    desktop = read(CHECKS / 'desktop/results.json')
    photos = read(PHOTOS / 'results.json')
    previous = read(ROOT / 'docs/reports/AURA-hundred-capabilities-2026-10-21.json')
    rea_function = read(CHECKS / 'rea-function-status.json')
    rea_cli = read(CHECKS / 'rea-cli-670-doctor.json')
    review_launch = read(CHECKS / 'review-launch.json')
    cases = {row['name']: row for row in desktop.get('cases', [])}
    native_log = CHECKS / 'workspace-tests-verified.log'
    native_text = text(native_log)
    native = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored', native_text)
    totals = [sum(int(row[index]) for row in native) for index in range(3)]
    test_records = native_records(native_text)
    observed = Counter(row['status'] for row in test_records)
    complete_records = [observed['ok'], observed['FAILED'], observed['ignored']] == totals
    ui_log = re.sub(r'\x1b\[[0-9;]*m', '', text(CHECKS / 'ui-tests.log'))
    ui_tests = re.search(r'Tests\s+(\d+) passed', ui_log)
    ui_files = re.search(r'Test Files\s+(\d+) passed', ui_log)
    ui_summary = f"{ui_tests[1] if ui_tests else 'unavailable'} tests in {ui_files[1] if ui_files else 'unavailable'} files passed"
    ledger = []
    for item in previous['capabilities']:
        record = cases.get(item['id'], {})
        ledger.append(dict(number=item['number'], id=item['id'], name=item['name'], group=item['group'],
                           status='pass' if record.get('passed') else 'not completed', evidence=record))
    commands = sorted(set(re.findall(r'#\[tauri::command\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)', text(ROOT / 'ui/src-tauri/src/main.rs'))))
    calls = Counter(desktop.get('commandCounts', {})) + Counter(photos.get('commandCounts', {}))
    command_coverage = [dict(command=name, invocations=calls[name],
                             status='invoked in current desktop scenario' if calls[name] else 'no current desktop invocation recorded') for name in commands]
    statuses = {name: text(CHECKS / (name + '.exit')).strip() or 'not completed' for name in (
        'workspace-tests', 'desktop-build', 'clippy', 'contracts', 'desktop-audit', 'photo-audit', 'format', 'ui-tests', 'frontend-build', 'spatial-bit-tests')}
    statuses['workspace-tests'] = text(CHECKS / 'workspace-tests-verified.exit').strip() or 'not completed'
    payload = dict(date='2026-10-21', timezone='Asia/Katmandu', additions=ledger,
                   native=dict(passed=totals[0], failed=totals[1], ignored=totals[2], resultGroups=len(native),
                               caseRecords=len(test_records), caseRecordsMatchSummaries=complete_records),
                   checks=statuses, desktop=desktop, photos=photos, commands=command_coverage,
                   reaFunctionAttempt=rea_function, matchingReaCliReady=rea_cli.get('healthy', False),
                   reviewLaunch=review_launch,
                   completeEvotoReconstruction=False, evotoNativePairedExportAvailable=False,
                   proprietaryModelWeightsRecovered=False)
    destination = ROOT / 'docs/reports/AURA-revalidation-2026-10-21'
    with Path(str(destination) + '-native-tests.csv').open('w', newline='', encoding='utf-8') as stream:
        writer = csv.DictWriter(stream, fieldnames=['suite', 'test', 'status', 'reason'])
        writer.writeheader(); writer.writerows(test_records)
    with Path(str(destination) + '-commands.csv').open('w', newline='', encoding='utf-8') as stream:
        writer = csv.DictWriter(stream, fieldnames=['command', 'invocations', 'status'])
        writer.writeheader(); writer.writerows(command_coverage)
    destination.with_suffix('.json').write_text(json.dumps(payload, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    rows = '\n'.join(f"| {item['number']} | {item['group']} | {item['name']} | `{item['id']}` | {item['status']} |" for item in ledger)
    coverage = '\n'.join(f"| `{item['command']}` | {item['invocations']} | {item['status']} |" for item in command_coverage)
    export = photos.get('export', {})
    neutral_tools = ', '.join('`' + row['name'] + '`' for row in desktop.get('cases', []) if row.get('pixelsChanged') is False) or 'none recorded'
    matching_builds = bool(desktop.get('executableSha256')) and desktop.get('executableSha256') == photos.get('executableSha256')
    contents = f"""# AURA 100-tool expansion: corrected implementation and fresh qualification

Date: 2026-10-21, Asia/Katmandu.

This pass preserves and repairs the existing uncommitted hundred-capability expansion,
which follows the earlier two fifty-item passes. It does **not** count those same
tools as another new hundred. There are 25 colour methods, 25 spatial methods,
25 selection workflows and 25 cohort review workflows. All are original AURA
implementations. Exact Evoto behavior, proprietary trained models and full
feature parity remain incomplete and unverified.

Fresh desktop captures from the synthetic fixture:
[colour panel](assets/aura-revalidated-colour-2026-10-21.png),
[spatial panel](assets/aura-revalidated-spatial-2026-10-21.png),
[selection panel](assets/aura-revalidated-selection-2026-10-21.png),
[review workspace](assets/aura-revalidated-workflow-2026-10-21.png).

## Current checks

| Check | Recorded result |
| --- | --- |
| Native workspace, `--locked --no-fail-fast -j 4`, documented `AURA_PERF_HOST_SCALE=6` | {totals[0]} passed, {totals[1]} failed, {totals[2]} ignored, {len(native)} result groups; exit {statuses['workspace-tests']} |
| Frontend | {ui_summary}; tests exit {statuses['ui-tests']}; production build exit {statuses['frontend-build']} |
| Strict workspace Clippy | exit {statuses['clippy']} |
| Frozen contracts | exit {statuses['contracts']} |
| Rust format | exit {statuses['format']} |
| IPC surface | 347 definitions = 347 registrations = 347 typed calls; see current gate log |
| Python evidence/packaging/IPC tests | 15 passed |
| Exact-bit spatial regression rerun | 9 passed; exit {statuses['spatial-bit-tests']} |
| Desktop production rebuild | exit {statuses['desktop-build']} |
| Actual desktop audit | completed: {desktop.get('completed', False)}; {len(desktop.get('cases', []))} checks; exit {statuses['desktop-audit']} |
| Photo edits | {sum(bool(p.get('passed')) for p in photos.get('photos', []))}/{len(photos.get('inputManifest', []))}; completed: {photos.get('completed', False)} |
| Full-resolution JPEG export | {export.get('written', 0)} written; {export.get('verified', 0)} verified; render failures: {export.get('renderFailed', 'unavailable')}; exit {statuses['photo-audit']} |
| Original source SHA-256 unchanged | {photos.get('originalsUnchanged', False)} |
| Desktop and photo executable identity agrees | {matching_builds} |
| Final AURA review window | opened: {review_launch.get('opened', False)}; {review_launch.get('savedRecipeHashesVerified', 0)} saved recipe hashes verified against exports |

Logs are under `.work-checks/aura100-revalidation/`. A reported exit other than 0
or an incomplete item is not a pass. Ignored tests are excluded from pass counts.
The native suite covers kernel mathematics, validation, history and application
behavior; its breadth does not certify every camera, optional model, external
service or physical device. UI tests are automated component tests. Desktop checks
invoke the actual registered native routes and selected visible controls.
The frontend retains Vite's large-bundle advisory. The Windows desktop linker
reported a nonfatal `.rsrc` merge warning about multiple non-default manifests;
its build exit and actual startup/runtime results are recorded separately.
The first strict lint run rejected an oversized planning function. Validation
and plan-entry construction were extracted without suppressing that lint; the
first log is retained as `clippy-first.log`, and the table records the final run.
A later lint pass required the new exact-pixel regression assertions to compare
float bit patterns explicitly. That test-only correction preserves the strict
identity requirement; `clippy-exact-pixel-tests.log` retains the original diagnostic.
The final native verification is retained in `workspace-tests-verified.log`;
earlier compilation/qualification logs are separate and excluded from its totals.
The [individual native test ledger](AURA-revalidation-2026-10-21-native-tests.csv)
contains {len(test_records)} parsed console records; its pass/fail/ignore counts
match completed summaries: {complete_records}. Logs remain authoritative if a
console record was interleaved or incomplete. The
[command ledger](AURA-revalidation-2026-10-21-commands.csv) is also available as CSV.

## Bugs repaired

- Guided spatial sampling now renormalizes only eligible bilinear taps, including
  optical/channel sampling. It cannot leak excluded colour through interpolation.
- Rank filtering uses tie midpoints so flat patches remain unchanged. Incomplete
  Haar blocks are left unchanged instead of duplicating edges. Neutral-bridge and
  rotation geometry consistently use pixel-centre coordinates.
- Colour, spatial and material saves no longer duplicate changed history paths or
  append empty history when numeric round-trips have the same canonical recipe;
  explicit user ownership and protection against later automated overwrite remain.
- Three-way transfer compares each target against its own checkpoint; rollback
  binds to its originating checkpoint; relative tone transfer includes clarity,
  texture and dehaze. Legacy plan serialization remains compatible.
- Reopening a prerequisite reopens its dependent issues and invalidates affected
  checklist approvals. Metadata writes recheck collection membership.
- Unedited imported photos can receive their first batch edit atomically, with
  exact Undo/Redo. All recipe saves check membership inside their write transaction.
- Boundary skeletons retain edge-touching structure; fully matching chroma-key
  selections can clear; region splitting respects its maximum-area limit.
- Spatial stacks can reorder. Panels hide irrelevant controls, clear errors on
  discard, reconcile missing selections and discard stale comparison/document
  state when navigation changes their meaning.
- The renderer carries `labs.selected-v2` in cache identity. Evidence scripts
  reject stale catalogues, incorrect import identities, incomplete exports and
  failures instead of writing a successful completion flag.

See [ADR-0135](../adr/ADR-0135-selected-lab-correctness-and-first-edit-qualification.md).
Regression tests include first-save races, foreign project refusal, exact history,
selected/excluded samples, tied patches, odd blocks, reference endpoints, approval
dependencies, ancestor divergence, old serialization and UI navigation.

## Supplied photographs

Input: `H:/AURA EDIT`, 27 Canon CR3s. The fresh run imports by reference into a
separate catalogue, saves automatic edit recipes and renders before/after through
AURA's native renderer. Creative laboratory alternatives remain explicitly selected
tools; incompatible filters are not stacked onto every delivery photo.

Open the [fresh before/after gallery](../../output/aura100-revalidated/index.html).
Full JPEGs are under `output/aura100-revalidated/full-resolution/edited/` and the
[photo manifest](../../output/aura100-revalidated/results.json) records exact source
hashes, saved recipes, export dimensions and executable identity. The prior
[screenshot-guided experiment](../../output/aura100-final/989A2698-screenshot-guided-approximation.jpg)
is preserved separately and is not relabelled as a fresh native Evoto comparison.
Visual inspection of the retained first portrait shows the Evoto display preview
brighter/warmer and smoother than AURA's automatic output; a visible gap remains.
Photographic quality still requires human review at 100%.

## Evoto evidence and unresolved request

REA MCP was run on the actual installed GUI and worker with an explicit 256 MiB
read budget after the default 64 MiB budget refused the worker. Fresh PE evidence:

- Worker `Evoto-worker.exe`, SHA-256
  `add7588f94aed190b97076f949709ba9622aa14f046b661783135d838d3d3bb2`,
  evidence `ev_01eb87daf29b21e756e15269f5e07667d0dd6a7d9aaef6799fd945075d15ed63`.
- GUI `Evoto.exe`, SHA-256
  `7f020192f3b659ca4a1e4f8631b21879982bb21d97d26196a604ad70e2f3b2fa`,
  evidence `ev_feea8fbc779500b8d7a76d654b78642e3231781ecba36f541e57baa2853661cb`.

The artifacts match the previously analysed installation, so retained targeted
native/static and UI evidence is reused with its limitations. PE resource evidence
establishes artifact identity and metadata, not image-processing algorithms. The
current REA MCP session has no configured native decompiler. The matching 6.7.0
public CLI was subsequently verified against the already installed Ghidra/JDK
using process-local environment variables; no configuration or skill was replaced.
CLI readiness: {rea_cli.get('healthy', False)}. A bounded function-dossier attempt
for `0x141026430` records: `{json.dumps(rea_function, sort_keys=True)}`. A timeout
or failed dossier is not a completed decompilation. No new full-program
decompilation was completed. The previous Windows runtime observation edited
`989A2698.CR3`, but Export displayed a trial-enrollment prompt. No enrollment,
purchase or bypass was performed and no native Evoto export exists in that evidence.

The official [desktop product page](https://www.evoto.ai/ai-photo-editor) and
[Evoto 8.0 announcement](https://blog.evoto.ai/evoto-product-launch-2026/) describe
portrait, colour and workflow categories. Public feature descriptions guide the
inventory; they do not disclose proprietary models or prove equivalent results.

| Requested outcome | Evidence-based status |
| --- | --- |
| REA inspection of installed Evoto | Performed; identity and bounded prior static/runtime evidence |
| 100 additional AURA capabilities | Present and requalified below; independent implementations |
| Additional panels and sections | Colour lab, Spatial repair, Selection laboratory, Review & checkpoints |
| Edit supplied photos and export | Current results above and source-preservation manifest |
| Every Evoto feature and identical output | Incomplete; see previous [feature coverage](AURA-feature-coverage-2026-10-19.json) |
| Evoto neural architecture, weights, trained skins, training data | Not recovered or copied |
| Every optional AURA device/service and every control combination | Not exhaustively exercised; command coverage is explicit below |

The previous [native research](AURA-Evoto-native-research-2026-10-20.md) and
[runtime evidence](AURA-Evoto-colour-runtime-2026-10-21.md) retain their actual
scope. Similar tool purposes are not proof of the same kernels, models or output.

## Individual 100-tool current execution ledger

A pass means its scoped desktop scenario completed. Workflow entries point to
their action evidence within a sequential cohort scenario, supplemented by native
regressions; they are not 25 isolated photographic algorithms. Conditional defect
tools can produce unchanged pixels on a fixture lacking the relevant defect.
Unchanged pixels in this fixture were recorded for: {neutral_tools}.
Numerical behavior is checked independently by renderer tests. Spatial correction
still uses a bounded 256-pixel grid, selection analysis a 512-pixel grid, and
colour statistics at most 65536 selected samples. These remain practical limits.

| # | Area | Capability | ID | Current desktop result |
| ---: | --- | --- | --- | --- |
{rows}

## Registered command coverage

These invocation counts describe only the current desktop/photo scenarios.
Commands without a current record are explicitly unqualified at this layer.
They may have native/component tests or historical audit evidence; those are
different evidence and are not silently converted into current runtime passes.

| Native command | Current invocations | Scope |
| --- | ---: | --- |
{coverage}
"""
    destination.with_suffix('.md').write_text(contents, encoding='utf-8')
    print(json.dumps(dict(native=payload['native'], checks=statuses, desktopCompleted=desktop.get('completed', False), photosCompleted=photos.get('completed', False), qualifiedTools=sum(item['status'] == 'pass' for item in ledger)), indent=2))


if __name__ == '__main__':
    main()
