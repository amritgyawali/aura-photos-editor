"""Build a ledger of distinct additions; never infer verification from implementation."""
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'docs/reports/AURA-capabilities-2026-10-22.json'


def build():
    capabilities = []
    for group in ['colour', 'spatial', 'selection']:
        path = ROOT / f'docs/reports/AURA-{group}-additions-2026-10-22.json'
        data = json.loads(path.read_text(encoding='utf-8'))
        rows = data if isinstance(data, list) else next((data[key] for key in ['methods', 'capabilities', 'tools', 'workflows', 'features'] if key in data), [])
        if len(rows) != 25:
            raise ValueError(f'{group}: expected 25 methods, got {len(rows)}')
        for row in rows:
            tool = row['id']
            capabilities.append(dict(row, id=tool, name=row.get('name', row.get('label', row.get('title', tool))), group=group,
                                     implementation='Independent AURA implementation; Evoto numerical parity unverified.',
                                     status='implemented_pending_qualification'))
    for path in sorted((ROOT / 'crates/aura-app/src/photo_analysis/delivery').glob('*.rs')):
        if path.name == 'tests.rs':
            continue
        for match in re.finditer(r'\(\s*"([a-z][a-z-]+)",\s*"([^"]+)",\s*"([^"]+)"', path.read_text(encoding='utf-8')):
            tool, name, description = match.groups()
            groups = {
                'Delivery statistics': ['robust-tone-quantiles','median-deviation','luminance-inequality','hue-concentration','tone-hue-drift','highlight-headroom','chroma-tail','exposure-locations','highlight-rolloff','gamut-excursion-energy'],
                'Surface review': ['local-colour-jumps','structure-coherence','hessian-signature','gradient-tailedness','edge-polarity','normalized-total-variation','dct-energy','chroma-edge-alignment','residual-asymmetry','colour-variogram'],
                'Lighting geometry': ['local-bit-entropy','illumination-plane','illumination-curvature','edge-euler','edge-box-dimension'],
            }
            review_group = next(name for name, ids in groups.items() if tool in ids)
            capabilities.append(dict(id=tool, name=name, description=description, mechanism=description, group='inspection', reviewGroup=review_group,
                                     source=path.relative_to(ROOT).as_posix(), nativeCommand='inspect_photo_diagnostics',
                                     status='implemented_pending_qualification'))
    if len(capabilities) != 100 or len({row['id'] for row in capabilities}) != 100:
        raise ValueError(f'Expected 100 distinct additions; found {len(capabilities)}')
    historical = []
    for filename in ['AURA-hundred-capabilities-2026-10-21.json', 'AURA-next-hundred-capabilities-2026-10-21.json']:
        historical.extend(json.loads((ROOT / 'docs/reports' / filename).read_text(encoding='utf-8'))['capabilities'])
    earlier = {(row['group'], row['id']) for row in historical}
    for row in capabilities:
        if (row['group'], row['id']) in earlier:
            raise ValueError('Earlier capability recounted: ' + row['id'])
    result = dict(date='2026-10-22', count=100, groups={group:sum(row['group']==group for row in capabilities) for group in ['colour','spatial','selection','inspection']},
                  exactEvotoParity=False, proprietaryModelsRecovered=False, capabilities=capabilities,
                  limitations=['50 rendering methods, 25 selection operations, 25 read-only diagnostics; no new trained models.',
                               'Native, desktop, and real-photo results must be read from the validation report.',
                               'All parameter combinations and subjective visual perfection are not established by automated tests.'])
    OUTPUT.write_text(json.dumps(result, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
    print(json.dumps(dict(count=100, groups=result['groups'], output=str(OUTPUT))))


if __name__ == '__main__':
    build()
