"""Build an auditable registry of the next100 additions, excluding the earlier100."""
import json
from pathlib import Path
import re

ROOT=Path(__file__).resolve().parents[1]
EVIDENCE=ROOT/'.work-checks/aura-next100'
OUTPUT=ROOT/'docs/reports/AURA-next-hundred-capabilities-2026-10-21.json'


def build():
    capabilities=[]
    previous=json.loads((ROOT/'docs/reports/AURA-hundred-capabilities-2026-10-21.json').read_text(encoding='utf-8'))['capabilities']
    for group in ['colour','spatial','selection']:
        rows=json.loads((EVIDENCE/f'{group}-ledger.json').read_text(encoding='utf-8'))
        if len(rows)!=25:
            raise ValueError(f'{group}: expected25 distinct additions')
        old={item['id'] for item in previous if item['group']==group}
        for row in rows:
            if row['id'] in old:
                raise ValueError('Earlier addition recounted: '+row['id'])
            capabilities.append(dict(row, group=group, implementation='Independent AURA mathematics; no proprietary model recovery or output parity claimed.', status='implemented_pending_qualification'))
    for path in sorted((ROOT/'crates/aura-app/src/photo_analysis/quality').glob('*.rs')):
        if path.name=='tests.rs':
            continue
        for match in re.finditer(r'\(\s*"([a-z][a-z-]+)",\s*"([^"]+)",\s*"([^"]+)"',path.read_text(encoding='utf-8')):
            tool_id,title,description=match.groups()
            capabilities.append(dict(id=tool_id,name=title,description=description,math=description,
                                     group='inspection',source=path.relative_to(ROOT).as_posix(),
                                     command='inspect_photo_diagnostics',
                                     test='photo_analysis::quality::tests',
                                     implementation='Independent bounded read-only proxy measurement; no learned classification.',status='implemented_pending_qualification'))
    if len(capabilities)!=100 or len({row['id'] for row in capabilities})!=100:
        raise ValueError(f'Expected100 distinct capabilities, found {len(capabilities)}')
    if sum(row['group']=='inspection' for row in capabilities)!=25:
        raise ValueError('Expected25 new inspection tools')
    for row in capabilities:
        if not (ROOT/row['source']).is_file():
            raise ValueError('Missing implementation: '+row['source'])
    output=dict(date='2026-10-21',scope='100 additional capabilities after the previous hundred',
                count=100,groups={name:sum(row['group']==name for row in capabilities) for name in ['colour','spatial','selection','inspection']},
                exactEvotoParity=False,proprietaryModelsRecovered=False,capabilities=capabilities,
                limitations=['50 new render methods,25 new selection workflows,and25 new read-only diagnostics; not100 newly trained AI models.',
                             'Spatial tools operate on a bounded256-pixel correction grid; diagnostics use a512-pixel proxy.',
                             'Desktop and RAW qualification must be read from the accompanying final report, not inferred from this registry.'])
    OUTPUT.write_text(json.dumps(output,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    print(json.dumps(dict(count=len(capabilities),groups=output['groups'],output=str(OUTPUT))))


if __name__=='__main__':
    build()
