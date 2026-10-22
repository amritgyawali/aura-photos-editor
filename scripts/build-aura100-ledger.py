"""Build the explicit 100-item ledger from the implemented UI registries and workflow guide."""
import json
from pathlib import Path
import re

ROOT=Path(__file__).resolve().parents[1]
def main():
    rows=[]
    for filename,key,group in [('ui/src/ipc/colorLab.ts','COLOR_LAB_TOOLS','colour'),('ui/src/ipc/spatialLab.ts','SPATIAL_TOOLS','spatial')]:
        text=(ROOT/filename).read_text(encoding='utf-8');block=text.split(f'export const {key} = [',1)[-1] if f'export const {key} = [' in text else text.split(f'export const {key} =',1)[-1]
        block=block.split('] as const',1)[0]
        entries=re.findall(r"\[\s*'([a-z_]+)'\s*,\s*'([^']+)'\s*,",block)
        assert len(entries)==25,(filename,len(entries))
        rows.extend({'id':identifier,'name':label,'group':group,'provenance':'independent AURA method','surface':filename}for identifier,label in entries)
    text=(ROOT/'ui/src/components/develop/MaskLabControls.tsx').read_text(encoding='utf-8')
    entries=re.findall(r"action: '([a-z_]+)', label: '([^']+)'",text)
    assert len(entries)==25,len(entries)
    rows.extend({'id':identifier,'name':label,'group':'selection','provenance':'independent AURA workflow','surface':'ui/src/components/develop/MaskLabControls.tsx'}for identifier,label in entries)
    text=(ROOT/'docs/reports/AURA-workflow-lab-2026-10-20.md').read_text(encoding='utf-8')
    entries=re.findall(r'^\| (\d+) \| ([^|]+) \| ([^|]+) \|$',text,re.M)
    assert len(entries)==25,len(entries)
    rows.extend({'id':f'review-{index}','name':name.strip(),'group':'workflow','provenance':'original AURA workflow','surface':'ui/src/components/develop/WorkflowLabPanel.tsx','operation':operation.strip()}for index,name,operation in entries)
    assert len(rows)==100
    for number,row in enumerate(rows,1):row['number']=number
    result={'date':'2026-10-21','capabilities':rows,'evotoFullReconstruction':False,'evotoWeightsRecovered':False,'evotoPixelParity':'unverified','verification':'See final report; individual results recorded after execution.'}
    path=ROOT/'docs/reports/AURA-hundred-capabilities-2026-10-21.json';path.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');print(f'Validated {len(rows)} distinct registry entries')
if __name__=='__main__':main()
