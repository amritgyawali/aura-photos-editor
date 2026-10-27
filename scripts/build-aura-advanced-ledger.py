"""Merge100 new independent mechanisms, rejecting IDs counted in prior reports."""
import json
from pathlib import Path
import re

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'docs/reports/AURA-advanced-capabilities-2026-10-22.json'
GROUPS={
    'Texture fidelity':['ordinal-texture-entropy','sample-texture-entropy','quantized-run-lengths','cooccurrence-texture','bright-mass-lacunarity','harris-corner-response','local-plane-residual','line-spectral-flatness','neighbour-moran-index','robust-neighbour-roughness'],
    'Colour stability':['log-ratio-neutrality','white-patch-illuminant','gray-edge-illuminant','opponent-colourfulness','hue-gap-coverage','local-hue-winding','chroma-light-information','patch-chroma-dispersion'],
    'Delivery integrity':['tukey-tail-support','channel-tail-dependence','patch-mean-variance-fit','clipping-component-support','edge-clipping-overlap','robust-strip-drift','thumbnail-alias-sensitivity'],
}

def build():
    capabilities=[];fixes={}
    for group in ['colour','spatial','selection']:
        data=json.loads((ROOT/f'docs/reports/AURA-{group}-advanced-2026-10-22.json').read_text(encoding='utf-8'))
        rows=data if isinstance(data,list) else next((data[k] for k in ['methods','capabilities','tools','workflows','features'] if k in data),[])
        if len(rows)!=25:raise ValueError(f'{group}: expected25, got{len(rows)}')
        fixes[group]=data.get('fixes',[]) if isinstance(data,dict) else []
        for row in rows:
            capabilities.append(dict(row,group=group,name=row.get('name',row.get('label',row.get('title',row['id']))),status='implemented_pending_qualification',evotoParity='unverified'))
    for path in sorted((ROOT/'crates/aura-app/src/photo_analysis/advanced').glob('*.rs')):
        if path.name=='tests.rs':continue
        for match in re.finditer(r'\(\s*"([a-z][a-z-]+)",\s*"([^"]+)",\s*"([^"]+)"',path.read_text(encoding='utf-8')):
            tool,name,description=match.groups()
            group=next(label for label,ids in GROUPS.items() if tool in ids)
            capabilities.append(dict(id=tool,name=name,mechanism=description,group='inspection',reviewGroup=group,source=path.relative_to(ROOT).as_posix(),status='implemented_pending_qualification',evotoParity='unverified'))
    if len(capabilities)!=100 or len({(r['group'],r['id']) for r in capabilities})!=100:raise ValueError('Expected100 distinct mechanisms')
    prior=[]
    for name in ['AURA-hundred-capabilities-2026-10-21.json','AURA-next-hundred-capabilities-2026-10-21.json','AURA-capabilities-2026-10-22.json']:
        prior.extend(json.loads((ROOT/'docs/reports'/name).read_text(encoding='utf-8'))['capabilities'])
    previous={(r['group'],r['id']) for r in prior}
    repeated=[r['id'] for r in capabilities if (r['group'],r['id']) in previous]
    if repeated:raise ValueError('Earlier mechanisms recounted: '+str(repeated))
    result=dict(date='2026-10-22',iteration='advanced',count=100,groups={g:sum(r['group']==g for r in capabilities) for g in ['colour','spatial','selection','inspection']},capabilities=capabilities,existingToolFixes=fixes,exactEvotoParity=False,proprietaryModelsRecovered=False,
                limitations=['50 rendering methods,25 selections,25 read-only diagnostics; no recovered trained models.','Read validation results for measured functional coverage and residual limits.'])
    OUT.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    print(json.dumps(dict(count=result['count'],groups=result['groups'],output=str(OUT))))

if __name__=='__main__':build()
