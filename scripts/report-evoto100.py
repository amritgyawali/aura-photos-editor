"""Build the ADR-0150 report from the engine run, the real-window run and the visual review.

    python scripts/report-evoto100.py <engine-out> <gui-out> <review.json> <report.md>

* `<engine-out>/results.json` - `crates/aura-app/tests/evoto100_photos.rs`: every tool on the
  photographs that suit it, with how much changed and how much of it stayed in its region.
* `<gui-out>/results.json` - `scripts/test-evoto100-gui.py`: every control worked in the window.
* `review.json` - `{tool id: {"grade": "good"|"usable"|"weak"|"not working", "note": "..."}}`, written
  after looking at every before-and-after crop at full size.

Each tool gets one line: whether its control works in the window, what the engine measured on the
photographs, and the visual grade with what was seen. Nothing is called "the same as Evoto": that
cannot be measured without Evoto's own output on the same photographs.
"""
import json
import sys
from pathlib import Path


def main():
    engine_dir, gui_dir, review_path, out = map(Path, sys.argv[1:5])
    engine = json.loads((engine_dir / 'results.json').read_text(encoding='utf-8'))
    gui = json.loads((gui_dir / 'results.json').read_text(encoding='utf-8')) if (gui_dir / 'results.json').exists() else {'cases': []}
    review = json.loads(review_path.read_text(encoding='utf-8')) if review_path.exists() else {}
    window = {}
    for c in gui['cases']:
        for tid in str(c['id']).split('-'):
            window.setdefault(tid, []).append(c)

    lines = ['# A hundred Evoto-style tools on 27 real photographs - 2026-10-22', '',
             'Photographs: 27 Canon CR3 files of a Nepali wedding (`H:\\AURA EDIT`), imported by reference into an isolated '
             'catalogue; the originals were only read. Every tool was used two ways: through the application\'s own commands '
             'on the photographs that suit it (`crates/aura-app/tests/evoto100_photos.rs`), and by its control in the real '
             'desktop window (`scripts/test-evoto100-gui.py`). Each before-and-after was then looked at by eye.', '',
             '**What "changed" and "inside" mean.** *Changed* is the share of the frame that moved by three levels of 255 or more. '
             '*Inside* is the share of all the change that landed in the region the tool is for (faces for face tools, the '
             'background for background tools); 1.00 means none leaked out.', '',
             '**On "the same as Evoto".** Evoto\'s models and code were not opened or copied, and no Evoto output of these '
             'photographs was available, so sameness cannot be measured. Each tool is graded on whether it does its job well '
             'on these photographs.', '']
    grades = {}
    groups = {}
    for t in engine['tools']:
        groups.setdefault(t['group'], []).append(t)
    for group, tools in groups.items():
        lines += [f'## {group}', '', '| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |', '|---|---|---|---|---|---|']
        for t in tools:
            cases = window.get(t['id'], [])
            words = {'pass': 'works', 'warn': 'works, small change', 'fail': 'failed', 'skip': 'skipped'}
            win = ', '.join(words.get(s, s) for s in sorted({c['status'] for c in cases})) or 'engine only'
            measures = []
            for m in t['measures']:
                if m.get('note') and not m.get('changed'):
                    measures.append(f"{m['photo']}: {m['note'][:160]}")
                else:
                    inside = '-' if m.get('inside') is None else f"{m['inside']:.2f}"
                    measures.append(f"{m['photo']}: {m['changed'] * 100:.1f}%, {inside}, {m['ms']}")
            r = review.get(t['id'], {})
            grade = r.get('grade', 'not reviewed')
            grades[grade] = grades.get(grade, 0) + 1
            lines.append(f"| {t['id']} | {t['label']} | {win} | {'<br>'.join(measures)} | **{grade}** | {r.get('note', '')} |")
        lines.append('')
    summary = ', '.join(f'{k}: {v}' for k, v in sorted(grades.items(), key=lambda kv: -kv[1]))
    first_group = next(i for i, line in enumerate(lines) if line.startswith('## '))
    lines.insert(first_group, f'**Grades:** {summary}.')
    lines.insert(first_group + 1, '')
    out.write_text('\n'.join(lines), encoding='utf-8')
    print(out, summary)


if __name__ == '__main__':
    main()
